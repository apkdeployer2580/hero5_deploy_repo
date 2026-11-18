package kr.heroeslore.five;

import android.content.res.AssetFileDescriptor;
import android.content.res.AssetManager;
import android.media.AudioAttributes;
import android.content.Context;
import android.media.AudioManager;
import android.media.MediaPlayer;
import android.media.SoundPool;
import android.util.Log;

import java.util.ArrayList;
import java.util.HashMap;

/**
 * Sound effects and music, played by the platform's own decoders.
 *
 * <h2>Why this is Java and not Rust</h2>
 *
 * The clips live inside {@code assets/data.vfs.mp3} as Ogg Vorbis, and the
 * original game never decoded them itself: {@code MidasSoundManager.load} in
 * the shipped {@code classes.dex} handed {@code SoundPool} a byte range of that
 * asset and let Android do the work, and {@code MidasMusicManager} did the same
 * with {@code MediaPlayer}. Reproducing that is both less code and closer to
 * the original than a Vorbis decoder and mixer would be — same decoder, same
 * resampling, same looping behaviour.
 *
 * <p>Reaching {@code SoundPool.Builder} and {@code MediaPlayer} through raw JNI
 * from Rust would be a hundred lines of method-signature strings for something
 * the Java compiler checks for free, so the Android API usage lives here and
 * Rust calls the seven static methods below. That is the whole surface.
 *
 * <h2>The file descriptor — one per load, on purpose</h2>
 *
 * {@code AssetManager.openFd} only works on an asset stored uncompressed, which
 * is why the archive is named {@code .mp3} (aapt's default no-compress list)
 * and why {@code build.gradle.kts} says so explicitly as well. The descriptor
 * it returns points at the whole APK, so every offset the guest gives has
 * {@link AssetFileDescriptor#getStartOffset()} added to it — exactly the
 * {@code vfsOffset + offset} the original computed.
 *
 * <p>The original opened that descriptor <em>once</em> and handed the same
 * {@code FileDescriptor} to every {@code SoundPool.load} and every
 * {@code MediaPlayer.setDataSource}. That was fine on the 2011 platform and is
 * still fine on Android 10 (the Galaxy S9 the original is checked against),
 * where SoundPool decodes its samples one at a time on a single thread
 * ({@code SoundPoolThread}). It is <em>not</em> fine on Android 11 and later:
 * the rewritten SoundPool runs two decoder threads
 * ({@code kDecoderThreads = hardware_concurrency() >= 4 ? 2 : 1}), each
 * {@code Sound} holds a {@code dup()} of the descriptor it was given, and the
 * extractor underneath ({@code FileSource::readAt}) positions with
 * {@code lseek64} and then calls {@code read}. A {@code dup()} shares the open
 * file description — and therefore the <em>file offset</em> — with the
 * original and with every other dup, so two clips decoding at once seek the
 * same offset out from under each other and read each other's bytes. The guest
 * loads its 21 effects back to back at boot; the first pair to decode together
 * is {@code eff_00} and {@code eff_01}, the weapon swing and the weapon hit,
 * and a corrupted decode is a sample that plays as silence or a stub while
 * {@code SoundPool.play} still returns a stream id. The music player's own
 * dup, streaming from mediaserver, shares the same offset and races the
 * decoders the same way.
 *
 * <p>So every load opens its own {@code AssetFileDescriptor} — a fresh
 * {@code open()} of the APK, hence its own file offset — and closes it as soon
 * as the platform has taken its dup, which both {@code SoundPool.load} (the
 * dup happens inside the call) and {@code MediaPlayer.setDataSource} ("safe to
 * close as soon as this call returns") promise. One extra {@code open()} per
 * clip, and none of them can read another's bytes.
 *
 * <h2>Threading</h2>
 *
 * Every method here is called from the one Rust thread that runs the event
 * loop, once a frame. The maps are still synchronized because
 * {@code MediaPlayer}'s completion callback arrives on the main looper.
 */
public final class Hl5Audio {
    private static final String TAG = "hl5";

    /** Matches {@code hl5_core::audio::ClipKind}. */
    public static final int KIND_SOUND = 0;
    public static final int KIND_MUSIC = 1;

    private static AssetManager sAssets;
    private static String sVfsName;
    private static SoundPool sPool;

    /** Guest handle to SoundPool sound id. */
    private static final HashMap<Long, Integer> sSounds = new HashMap<>();
    /** Guest handle to the stream id of its most recent play, for stop/pause. */
    private static final HashMap<Long, Integer> sStreams = new HashMap<>();
    /** Guest handle to its MediaPlayer. */
    private static final HashMap<Long, MediaPlayer> sMusic = new HashMap<>();
    /** Tracks paused by {@link #suspend()}, to be restarted by {@link #restore()}. */
    private static final ArrayList<Long> sWasPlaying = new ArrayList<>();

    /** WIPI volumes, 0..100, as the guest last set them. */
    private static float sSoundVol = 1.0f;
    private static float sMusicVol = 1.0f;
    /**
     * {@code AudioManager.getStreamMaxVolume(STREAM_MUSIC)} — the device's own
     * step count for the media stream, and the scale the original's effect
     * volume was expressed in. See {@link #volume}.
     */
    private static float sStreamMax = 15.0f;

    private Hl5Audio() {}

    /**
     * Open the archive and build the pool. Returns false if the asset cannot be
     * opened as a descriptor, which leaves the game silent but running — the
     * Rust side logs it and stops sending events.
     */
    public static boolean init(Context ctx, AssetManager am, String vfsName) {
        if (sPool != null) {
            return true;
        }
        long base;
        try {
            // Opened once here only to prove it can be and to log where the
            // archive sits; every load opens its own (see the class comment).
            AssetFileDescriptor probe = am.openFd(vfsName);
            base = probe.getStartOffset();
            probe.close();
        } catch (Exception e) {
            // The likely cause is the asset having been compressed after all,
            // in which case openFd throws rather than returning a partial
            // descriptor. Worth saying plainly: it is a build problem, not a
            // device one.
            Log.e(TAG, "audio: cannot openFd(" + vfsName + ") - is it compressed in the APK?", e);
            return false;
        }
        sAssets = am;
        sVfsName = vfsName;
        AudioAttributes attrs = new AudioAttributes.Builder()
                .setUsage(AudioAttributes.USAGE_GAME)
                .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
                .build();
        // Eight streams is more than this game ever asks for at once; the
        // original used the SoundPool default and never ran out.
        sPool = new SoundPool.Builder().setMaxStreams(8).setAudioAttributes(attrs).build();
        try {
            AudioManager am2 = (AudioManager) ctx.getSystemService(Context.AUDIO_SERVICE);
            int max = am2.getStreamMaxVolume(AudioManager.STREAM_MUSIC);
            if (max > 0) {
                sStreamMax = max;
            }
        } catch (Exception e) {
            Log.w(TAG, "audio: no AudioManager; assuming streamMax " + sStreamMax, e);
        }
        Log.i(TAG, "audio: ready, archive starts at " + base + " in the APK"
                + " (streamMax " + (int) sStreamMax + ")");
        return true;
    }

    /** {@code putSoundClip} / {@code putMusicClip}: a byte range became a clip. */
    public static void load(long key, int kind, long offset, long length) {
        if (sPool == null || length <= 0) {
            return;
        }
        try {
            if (kind == KIND_SOUND) {
                // Asynchronous: the id is valid immediately but the sample is
                // not playable until SoundPool finishes decoding. The guest
                // loads everything during its boot and plays much later, so
                // this races only for a sound played in the same frame it was
                // loaded, which is what the original did too.
                //
                // Own descriptor per clip: SoundPool dups it inside load(),
                // and a dup shares the file offset with whatever it was
                // dup'ed from -- see the class comment for why that matters.
                AssetFileDescriptor afd = sAssets.openFd(sVfsName);
                int id;
                try {
                    id = sPool.load(afd.getFileDescriptor(), afd.getStartOffset() + offset, length, 1);
                } finally {
                    afd.close();
                }
                sSounds.put(key, id);
            } else {
                // Release the previous track under this handle before the new
                // one starts reading, rather than after: two live players
                // would otherwise overlap for the length of prepare().
                MediaPlayer old = sMusic.remove(key);
                if (old != null) {
                    old.release();
                }
                MediaPlayer mp = new MediaPlayer();
                mp.setAudioAttributes(new AudioAttributes.Builder()
                        .setUsage(AudioAttributes.USAGE_GAME)
                        .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
                        .build());
                AssetFileDescriptor afd = sAssets.openFd(sVfsName);
                try {
                    mp.setDataSource(afd.getFileDescriptor(), afd.getStartOffset() + offset, length);
                } finally {
                    // Documented as safe the moment setDataSource returns:
                    // mediaserver holds its own dup.
                    afd.close();
                }
                mp.prepare();
                mp.setVolume(sMusicVol, sMusicVol);
                sMusic.put(key, mp);
            }
        } catch (Exception e) {
            Log.w(TAG, "audio: load(" + key + ") failed", e);
        }
    }

    public static void play(long key, boolean looping) {
        try {
            MediaPlayer mp = sMusic.get(key);
            if (mp != null) {
                mp.setLooping(looping);
                // The guest restarts a track by calling play again, so seek
                // rather than resuming wherever the last stop left it.
                mp.seekTo(0);
                mp.setVolume(sMusicVol, sMusicVol);
                mp.start();
                return;
            }
            Integer id = sSounds.get(key);
            if (id != null && sPool != null) {
                // SoundPool's loop argument is a count, with -1 meaning
                // forever and 0 meaning play once.
                int stream = sPool.play(id, sSoundVol, sSoundVol, 1, looping ? -1 : 0, 1.0f);
                sStreams.put(key, stream);
                // 0 is SoundPool's failure return: the sample had not finished
                // decoding, or the id no longer names a loaded sample. Silent
                // effects were hard enough to find once; say it out loud.
                if (stream == 0) {
                    Log.w(TAG, "audio: SoundPool refused key=" + key + " id=" + id
                            + " vol=" + sSoundVol);
                }
            }
        } catch (Exception e) {
            Log.w(TAG, "audio: play(" + key + ") failed", e);
        }
    }

    public static void stop(long key) {
        try {
            MediaPlayer mp = sMusic.get(key);
            if (mp != null) {
                if (mp.isPlaying()) {
                    mp.pause();
                }
                // Rewound rather than stopped: MediaPlayer.stop() puts the
                // object back in the Stopped state, where start() throws until
                // it is prepared again, and the guest expects to be able to
                // play the same handle later.
                mp.seekTo(0);
                return;
            }
            Integer stream = sStreams.remove(key);
            if (stream != null && sPool != null) {
                sPool.stop(stream);
            }
        } catch (Exception e) {
            Log.w(TAG, "audio: stop(" + key + ") failed", e);
        }
    }

    public static void pause(long key) {
        try {
            MediaPlayer mp = sMusic.get(key);
            if (mp != null) {
                if (mp.isPlaying()) {
                    mp.pause();
                }
                return;
            }
            Integer stream = sStreams.get(key);
            if (stream != null && sPool != null) {
                sPool.pause(stream);
            }
        } catch (Exception e) {
            Log.w(TAG, "audio: pause(" + key + ") failed", e);
        }
    }

    public static void resume(long key) {
        try {
            MediaPlayer mp = sMusic.get(key);
            if (mp != null) {
                mp.start();
                return;
            }
            Integer stream = sStreams.get(key);
            if (stream != null && sPool != null) {
                sPool.resume(stream);
            }
        } catch (Exception e) {
            Log.w(TAG, "audio: resume(" + key + ") failed", e);
        }
    }

    public static void free(long key) {
        MediaPlayer mp = sMusic.remove(key);
        if (mp != null) {
            mp.release();
        }
        Integer stream = sStreams.remove(key);
        if (stream != null && sPool != null) {
            sPool.stop(stream);
        }
        Integer id = sSounds.remove(key);
        if (id != null && sPool != null) {
            sPool.unload(id);
        }
    }

    /**
     * WIPI volumes, 0..100. Applies to anything already playing.
     *
     * <p>The effect gain is <em>not</em> {@code sound / 100}. The original's
     * {@code MidasSoundManager} kept its effect volume in the device's own
     * stream-volume units and then handed that straight to {@code SoundPool}:
     *
     * <pre>
     *   volumeRateWIPI = getStreamMaxVolume(STREAM_MUSIC) / 100f
     *   soundVolume    = min(wipi * volumeRateWIPI, streamMax)
     *   soundPool.play(id, soundVolume, soundVolume, 1, loop, 1f)
     * </pre>
     *
     * {@code SoundPool.play} takes 0..1 and clamps, so on a typical handset
     * (streamMax 15) every WIPI value at or above ~7 saturated to full volume.
     * The game plays with {@code sound=10}, which the original therefore heard
     * at 1.5 -> 1.0, and a straight {@code 10/100 = 0.1} made every effect
     * ~20 dB quieter than the reference. Measured on the S25: effects were
     * being played (SoundPool returned live stream ids) and simply could not
     * be heard.
     *
     * <p>Master is kept as a local multiplier. The original pushed it into
     * {@code AudioManager.setStreamVolume}, i.e. it moved the phone's system
     * media volume; a re-host has no business doing that to the device.
     */
    public static void volume(int master, int sound, int music) {
        float m = clamp(master);
        sSoundVol = wipiGain(sound) * m;
        sMusicVol = wipiGain(music) * m;
        for (Integer stream : sStreams.values()) {
            if (sPool != null) {
                sPool.setVolume(stream, sSoundVol, sSoundVol);
            }
        }
        for (MediaPlayer mp : sMusic.values()) {
            try {
                mp.setVolume(sMusicVol, sMusicVol);
            } catch (Exception ignored) {
                // A released player throws; nothing here can act on it.
            }
        }
    }

    /**
     * The original's WIPI-volume-to-gain conversion, bug and all.
     *
     * <p>{@code MidasSoundManager} and {@code MidasMusicManager} both compute
     * {@code wipi * (getStreamMaxVolume(STREAM_MUSIC) / 100f)}, clamp it to
     * that same stream maximum, and then pass the result to an API whose range
     * is 0..1 ({@code SoundPool.play}, {@code MediaPlayer.setVolume}). The
     * value is in stream steps, so on a 15-step device anything from WIPI ~7
     * upward saturates to full volume. Dividing by 100 instead — the reading
     * the field names invite — leaves the game roughly 20 dB quiet, which is
     * what the S25 measured before this was traced back to the dex.
     */
    private static float wipiGain(int wipi) {
        if (wipi <= 0) {
            return 0.0f;
        }
        return Math.min(1.0f, wipi * (sStreamMax / 100.0f));
    }

    private static float clamp(int wipi) {
        if (wipi <= 0) {
            return 0.0f;
        }
        return wipi >= 100 ? 1.0f : wipi / 100.0f;
    }

    /**
     * Silence everything without forgetting it, for onPause. The guest is
     * stopped at the same moment, so it never asks for this itself; without it
     * the music keeps playing over whatever the user switched to.
     */
    public static void suspend() {
        if (sPool != null) {
            sPool.autoPause();
        }
        sWasPlaying.clear();
        for (HashMap.Entry<Long, MediaPlayer> e : sMusic.entrySet()) {
            try {
                if (e.getValue().isPlaying()) {
                    // Remembered here rather than in the Rust caller: this is
                    // the only place that knows whether the platform actually
                    // had the track running, as opposed to the guest believing
                    // it did.
                    sWasPlaying.add(e.getKey());
                    e.getValue().pause();
                }
            } catch (Exception ignored) {
            }
        }
    }

    /** The other half of {@link #suspend()}, for onResume. */
    public static void restore() {
        if (sPool != null) {
            sPool.autoResume();
        }
        for (Long key : sWasPlaying) {
            MediaPlayer mp = sMusic.get(key);
            if (mp != null) {
                try {
                    mp.start();
                } catch (Exception ignored) {
                }
            }
        }
        sWasPlaying.clear();
    }
}
