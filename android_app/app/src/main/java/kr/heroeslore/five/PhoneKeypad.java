package kr.heroeslore.five;

import android.app.Activity;
import android.graphics.Canvas;
import android.graphics.DashPathEffect;
import android.graphics.Paint;
import android.graphics.RectF;
import android.os.Build;
import android.util.Log;
import android.util.SparseIntArray;
import android.view.HapticFeedbackConstants;
import android.view.MotionEvent;
import android.view.View;
import android.view.ViewConfiguration;
import android.view.WindowInsets;
import android.widget.FrameLayout;

import java.io.BufferedReader;
import java.io.File;
import java.io.FileReader;
import java.io.FileWriter;
import java.io.IOException;
import java.io.Writer;
import java.util.Locale;

/**
 * The 세로 (portrait) keypad: 24 keys under a 3:4 game view, the way a feature
 * phone held its keys under its screen.
 *
 * <p>This view only draws and tells the native side which key went down or up;
 * what a key DOES is {@code hl5_core::portrait::KEYPAD}, and the index of a key
 * here is its index there — that table is the contract, so the order of
 * {@link #LABEL} must not change without it. The native side routes the keys
 * through the gamepad layer, which is why 2/4/6/8 walk in the field (they become
 * the game's own joystick) and move the cursor in menus.
 *
 * <pre>
 *    L    ↑    R   |  1   2   3
 *    ←   OK    →   |  4   5   6
 *         ↓  대시   |  7   8   9
 *    ⚙  CLR  저장   |  *   0   #
 * </pre>
 *
 * <p>⚙ (index 18, where the phone's ☎ was — a second map key) never reaches the
 * native side: it opens the layout editor, in which every key can be dragged
 * and pinched to a new size. The layout is {@code <files>/hl5_keypad.txt};
 * the grid above is what an absent file means.
 */
final class PhoneKeypad extends View {
    /** Row-major, six to a row; "" is a gap with no key. */
    private static final String[] LABEL = {
        "L", "↑", "R", "1", "2", "3",
        "←", "OK", "→", "4", "5", "6",
        "", "↓", "대시", "7", "8", "9",
        "설정", "CLR", "저장", "*", "0", "#",
    };
    private static final int COLS = 6;
    private static final int ROWS = 4;
    /** The keys that walk. A finger that slides off one onto a non-walking key
     *  inside the number block keeps walking instead of firing that key, so a
     *  thumb rolling between 2/4/6/8 never stops on 5. */
    private static final int[] WALK = {1, 4, 6, 8, 9, 11, 13, 16};
    private static final int OK = 7;
    /** Opens the editor. Drawn as a gear; never sent to the native side. */
    private static final int SETTINGS = 18;

    /** The grid: a navigation block of 40% of the width beside a number block
     *  of 60%, three columns each, four rows between these margins. */
    private static final float GRID_LEFT = 0.4f;
    private static final float GRID_TOP = 0.05f;
    private static final float GRID_BOTTOM = 0.04f;
    /** A key's size is its grid cell times this, within these bounds (and
     *  never larger than the keypad itself). */
    private static final float MIN_SCALE = 0.5f;
    private static final float MAX_SCALE = 3f;
    private static final String LAYOUT_FILE = "hl5_keypad.txt";

    /** The game view's shape: 240x320, the guest's portrait screen. */
    private static final int CANVAS_W = 240;
    private static final int CANVAS_H = 320;
    /** The most of the window the game may take, so the keypad keeps room. */
    private static final float GAME_MAX_SHARE = 0.61f;

    // ---- The layout: what hl5_keypad.txt holds ---------------------------
    /** Each key's centre, as a fraction of the keypad's width and height. */
    private final float[] fx = new float[LABEL.length];
    private final float[] fy = new float[LABEL.length];
    /** Each key's size, as a multiple of its grid cell. */
    private final float[] scale = new float[LABEL.length];
    /** Draw order, bottom first. Hit testing walks it backwards, so where two
     *  keys overlap the one on top is the one pressed. */
    private final int[] order = new int[LABEL.length];

    /** The grid cell of each key at the current size, margins taken off. */
    private final float[] cellW = new float[LABEL.length];
    private final float[] cellH = new float[LABEL.length];
    private final RectF[] boxes = new RectF[LABEL.length];
    private final RectF numberBlock = new RectF();
    private float textSize;
    private float corner;

    // ---- Playing ---------------------------------------------------------
    /** Pointer id -> key index, for every finger currently on a key. */
    private final SparseIntArray fingers = new SparseIntArray();
    /** How many fingers hold each key; a key is down while this is > 0. */
    private final int[] holds = new int[LABEL.length];
    /** The finger that came down on ⚙, if one is still on the glass. */
    private int settingsFinger = -1;

    // ---- Editing ---------------------------------------------------------
    private boolean editing;
    private EditPanel panel;
    /** The key under the fingers, and the finger that moves it. */
    private int dragKey = -1;
    private int dragPointer = -1;
    /** The second finger of a pinch on {@link #dragKey}. */
    private int pinchPointer = -1;
    /** A first finger that landed on no key: a pinch may still start from it. */
    private int sparePointer = -1;
    /** Finger minus key centre, so a key does not jump to the finger. */
    private float grabX;
    private float grabY;
    private float downX;
    private float downY;
    private boolean moved;
    private float pinchDist;
    private float pinchScale;

    private final Paint paint = new Paint(Paint.ANTI_ALIAS_FLAG | Paint.FILTER_BITMAP_FLAG);
    private final DashPathEffect dash;
    private final float slop;
    private final File layoutFile;

    private PhoneKeypad(Activity activity) {
        super(activity);
        for (int i = 0; i < boxes.length; i++) {
            boxes[i] = new RectF();
        }
        float dp = activity.getResources().getDisplayMetrics().density;
        dash = new DashPathEffect(new float[] {6 * dp, 4 * dp}, 0);
        slop = ViewConfiguration.get(activity).getScaledTouchSlop();
        // The shell's storage is getFilesDir()/files, beside hl5_resolution.txt.
        layoutFile = new File(new File(activity.getFilesDir(), "files"), LAYOUT_FILE);
        resetToGrid();
        load();
        setBackgroundColor(0xFF000000);
        setHapticFeedbackEnabled(true);
    }

    /**
     * Put the keypad under the game. {@code content} is the FrameLayout
     * GameActivity made to hold {@code surface}; both are re-laid-out whenever
     * the window changes size, so a rotation that lands late still ends up in
     * the right shape. The editor's panel sits over the game picture and is
     * only shown while editing.
     */
    static void install(Activity activity, FrameLayout content, View surface) {
        final PhoneKeypad pad = new PhoneKeypad(activity);
        final EditPanel panel = new EditPanel(activity, pad);
        pad.panel = panel;
        content.setBackgroundColor(0xFF000000);
        content.addView(pad, new FrameLayout.LayoutParams(1, 1));
        content.addView(panel, new FrameLayout.LayoutParams(1, 1));
        content.addOnLayoutChangeListener((v, l, t, r, b, ol, ot, or, ob) ->
                pad.arrange(surface, r - l, b - t));
    }

    /** The game view's box and the keypad's, from the window's size. */
    private void arrange(View surface, int w, int h) {
        if (w <= 0 || h <= 0) {
            return;
        }
        // Clear the camera cut-out (and a hair more), not a fixed share of the
        // width: the hole is where it is, whatever the phone's proportions.
        int top = Math.round(w * 0.03f);
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            WindowInsets insets = getRootWindowInsets();
            if (insets != null && insets.getDisplayCutout() != null) {
                top = Math.max(top,
                        insets.getDisplayCutout().getSafeInsetTop() + Math.round(w * 0.01f));
            }
        }
        // A whole number of device pixels per canvas pixel when the window can
        // afford one at least twice over — the same reason the landscape canvas
        // is chosen to divide the screen (CLAUDE.md, ANALYSIS.md §33) — and the
        // largest 3:4 box that fits otherwise.
        float maxH = Math.min((float) w * CANVAS_H / CANVAS_W, (h - top) * GAME_MAX_SHARE);
        int k = (int) Math.floor(maxH / CANVAS_H);
        int gameW;
        int gameH;
        if (k >= 2) {
            gameW = CANVAS_W * k;
            gameH = CANVAS_H * k;
        } else {
            gameH = Math.round(maxH);
            gameW = Math.min(w, Math.round(gameH * (float) CANVAS_W / CANVAS_H));
        }
        place(surface, (w - gameW) / 2, top, gameW, gameH);
        place(panel, (w - gameW) / 2, top, gameW, gameH);
        // The keypad takes everything under the game and nothing over it, so a
        // finger on the bottom row of the picture still reaches the picture.
        place(this, 0, top + gameH, w, h - top - gameH);
    }

    private static void place(View v, int x, int y, int w, int h) {
        FrameLayout.LayoutParams lp = (FrameLayout.LayoutParams) v.getLayoutParams();
        if (lp.width == w && lp.height == h && lp.leftMargin == x && lp.topMargin == y) {
            return;
        }
        lp.width = w;
        lp.height = h;
        lp.leftMargin = x;
        lp.topMargin = y;
        v.setLayoutParams(lp);
    }

    // ---- Layout ----------------------------------------------------------

    /** Every key back in its grid cell at its grid size, in index order. */
    private void resetToGrid() {
        float rowH = (1f - GRID_TOP - GRID_BOTTOM) / ROWS;
        for (int i = 0; i < LABEL.length; i++) {
            int col = i % COLS;
            int row = i / COLS;
            fx[i] = col < 3
                    ? (col + 0.5f) * GRID_LEFT / 3f
                    : GRID_LEFT + (col - 3 + 0.5f) * (1f - GRID_LEFT) / 3f;
            fy[i] = GRID_TOP + (row + 0.5f) * rowH;
            scale[i] = 1f;
            order[i] = i;
        }
    }

    /** Read hl5_keypad.txt over the grid. A line that does not parse, names a
     *  key twice or puts one outside the keypad is skipped, so a hand edit gone
     *  wrong costs that key its place and nothing else. */
    private void load() {
        boolean[] seen = new boolean[LABEL.length];
        int[] named = new int[LABEL.length];
        int n = 0;
        try (BufferedReader r = new BufferedReader(new FileReader(layoutFile))) {
            for (String line; (line = r.readLine()) != null; ) {
                line = line.trim();
                if (line.isEmpty() || line.startsWith("#")) {
                    continue;
                }
                String[] f = line.split("\\s+");
                if (f.length != 4) {
                    continue;
                }
                try {
                    int i = Integer.parseInt(f[0]);
                    float x = Float.parseFloat(f[1]);
                    float y = Float.parseFloat(f[2]);
                    float s = Float.parseFloat(f[3]);
                    if (i < 0 || i >= LABEL.length || LABEL[i].isEmpty() || seen[i]
                            || !(x >= 0f && x <= 1f && y >= 0f && y <= 1f)
                            || !(s >= MIN_SCALE && s <= MAX_SCALE)) {
                        continue;
                    }
                    fx[i] = x;
                    fy[i] = y;
                    scale[i] = s;
                    seen[i] = true;
                    named[n++] = i;
                } catch (NumberFormatException e) {
                    // Skip the line.
                }
            }
        } catch (IOException e) {
            return; // No file: the grid.
        }
        // The file lists keys bottom first; keys it does not name go under them.
        int k = 0;
        for (int i = 0; i < LABEL.length; i++) {
            if (!seen[i]) {
                order[k++] = i;
            }
        }
        for (int m = 0; m < n; m++) {
            order[k++] = named[m];
        }
    }

    /** Write the layout, every key, bottom first. Through a temporary file so
     *  a kill mid-write leaves the old layout rather than half a new one. */
    private void save() {
        File dir = layoutFile.getParentFile();
        if (dir == null) {
            return;
        }
        dir.mkdirs();
        File tmp = new File(dir, LAYOUT_FILE + ".tmp");
        try (Writer out = new FileWriter(tmp)) {
            out.write("# 세로 키패드 배치: 키번호 가로중심 세로중심 크기\n");
            out.write("# 중심은 키패드 폭·높이에 대한 비율, 크기는 기본 칸의 배수. 지우면 기본 배치.\n");
            for (int i : order) {
                if (!LABEL[i].isEmpty()) {
                    out.write(String.format(Locale.ROOT, "%d %.4f %.4f %.3f\n",
                            i, fx[i], fy[i], scale[i]));
                }
            }
        } catch (IOException e) {
            Log.w("hl5", "keypad: could not write " + tmp + ": " + e);
            return;
        }
        if (!tmp.renameTo(layoutFile)) {
            Log.w("hl5", "keypad: could not replace " + layoutFile);
        }
    }

    private static float clamp(float v, float lo, float hi) {
        return lo > hi ? (lo + hi) / 2f : Math.max(lo, Math.min(hi, v));
    }

    /** The largest size key {@code i} may take: {@link #MAX_SCALE}, or as much
     *  of the keypad as its cell's shape allows. */
    private float maxScale(int i) {
        if (cellW[i] <= 0f || cellH[i] <= 0f) {
            return MAX_SCALE;
        }
        return Math.max(MIN_SCALE,
                Math.min(MAX_SCALE, Math.min(getWidth() / cellW[i], getHeight() / cellH[i])));
    }

    /** Key {@code i}'s box from its centre and size, kept on the keypad. The
     *  stored centre is left alone: a size that is only passing through (a
     *  rotation landing late) must not move a saved key. */
    private void fit(int i) {
        int w = getWidth();
        int h = getHeight();
        float s = clamp(scale[i], MIN_SCALE, maxScale(i));
        float hw = cellW[i] * s / 2f;
        float hh = cellH[i] * s / 2f;
        float x = clamp(fx[i] * w, hw, w - hw);
        float y = clamp(fy[i] * h, hh, h - hh);
        boxes[i].set(x - hw, y - hh, x + hw, y + hh);
    }

    private void fitAll() {
        if (getWidth() <= 0 || getHeight() <= 0) {
            return;
        }
        for (int i = 0; i < LABEL.length; i++) {
            fit(i);
        }
        numberBlock.setEmpty();
        for (int i = 0; i < LABEL.length; i++) {
            if (inNumberBlock(i) && !LABEL[i].isEmpty()) {
                numberBlock.union(boxes[i]);
            }
        }
    }

    @Override
    protected void onSizeChanged(int w, int h, int ow, int oh) {
        releaseAll();
        endGesture();
        float margin = w * 0.014f;
        float rowH = h * (1f - GRID_TOP - GRID_BOTTOM) / ROWS;
        for (int i = 0; i < LABEL.length; i++) {
            float cw = i % COLS < 3 ? w * GRID_LEFT / 3f : w * (1f - GRID_LEFT) / 3f;
            cellW[i] = cw - 2 * margin;
            cellH[i] = rowH - 2 * margin;
        }
        textSize = Math.min(w / 23f, h / 13f);
        corner = h / 40f;
        fitAll();
    }

    // ---- Drawing ---------------------------------------------------------

    @Override
    protected void onDraw(Canvas c) {
        super.onDraw(c);
        if (editing) {
            c.drawColor(0xFF0B1119);
        }
        paint.setTextAlign(Paint.Align.CENTER);
        for (int i : order) {
            if (LABEL[i].isEmpty()) {
                continue;
            }
            RectF b = boxes[i];
            float s = b.height() / Math.max(1f, cellH[i]);
            float r = corner * s;
            boolean down = holds[i] > 0;
            boolean held = editing && i == dragKey;
            paint.setStyle(Paint.Style.FILL);
            paint.setColor(down || held ? 0xFF302814 : 0xFF080B10);
            c.drawRoundRect(b, r, r, paint);
            paint.setStyle(Paint.Style.STROKE);
            if (editing) {
                // Dashed: these keys are things to move, not to press.
                paint.setPathEffect(dash);
                paint.setStrokeWidth(held ? 2.5f : 1.5f);
                paint.setColor(held ? 0xFFD5B34B : 0xFF8A95A6);
            } else {
                paint.setStrokeWidth(i == OK ? 2.5f : 1.5f);
                paint.setColor(i == OK || down ? 0xFFAA8734 : 0xFF657080);
            }
            c.drawRoundRect(b, r, r, paint);
            paint.setPathEffect(null);
            paint.setStyle(Paint.Style.FILL);
            int ink = i == OK || (editing && i == SETTINGS) ? 0xFFD5B34B : 0xFFA4A9B3;
            if (i == SETTINGS) {
                drawGear(c, b.centerX(), b.centerY(),
                        Math.min(b.width(), b.height()) * 0.27f, ink, 0xFF080B10);
            } else {
                paint.setColor(ink);
                paint.setTextSize(textSize * s);
                float y = b.centerY() - (paint.ascent() + paint.descent()) / 2f;
                c.drawText(LABEL[i], b.centerX(), y, paint);
            }
        }
        if (editing && dragKey >= 0 && pinchPointer >= 0) {
            // The size, over the key being pinched.
            RectF b = boxes[dragKey];
            paint.setColor(0xFFD5B34B);
            paint.setTextSize(textSize * 0.8f);
            float y = b.top - paint.descent() - textSize * 0.2f;
            if (y + paint.ascent() < 0) {
                y = b.bottom - paint.ascent() + textSize * 0.2f;
            }
            int pct = Math.round(b.height() / Math.max(1f, cellH[dragKey]) * 100);
            c.drawText(pct + "%", b.centerX(), y, paint);
        }
    }

    /** A gear: eight teeth round a disc with a hole in it. */
    private void drawGear(Canvas c, float x, float y, float r, int ink, int hole) {
        paint.setColor(ink);
        paint.setStyle(Paint.Style.STROKE);
        paint.setStrokeCap(Paint.Cap.BUTT);
        paint.setStrokeWidth(r * 0.42f);
        for (int t = 0; t < 8; t++) {
            double a = t * Math.PI / 4;
            float dx = (float) Math.cos(a) * r;
            float dy = (float) Math.sin(a) * r;
            c.drawLine(x + dx * 0.55f, y + dy * 0.55f, x + dx, y + dy, paint);
        }
        paint.setStyle(Paint.Style.FILL);
        c.drawCircle(x, y, r * 0.72f, paint);
        paint.setColor(hole);
        c.drawCircle(x, y, r * 0.3f, paint);
    }

    // ---- Playing ---------------------------------------------------------

    private int hit(float x, float y) {
        for (int n = order.length - 1; n >= 0; n--) {
            int i = order[n];
            if (!LABEL[i].isEmpty() && boxes[i].contains(x, y)) {
                return i;
            }
        }
        return -1;
    }

    private static boolean walks(int key) {
        for (int k : WALK) {
            if (k == key) {
                return true;
            }
        }
        return false;
    }

    /** The phone's number block: 1..9, *, 0, #. */
    private static boolean inNumberBlock(int key) {
        return key >= 0 && key % COLS >= 3;
    }

    private void press(int key, boolean down) {
        if (key < 0) {
            return;
        }
        if (down) {
            if (holds[key]++ > 0) {
                return;
            }
            performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP);
        } else {
            if (holds[key] == 0 || --holds[key] > 0) {
                return;
            }
        }
        if (key != SETTINGS) {
            MainActivity.nativeKeypad(key, down);
        }
        invalidate();
    }

    /** Move one finger onto {@code key}, releasing whatever it was on. */
    private void track(int pointer, int key) {
        int was = fingers.get(pointer, -1);
        if (was == key) {
            return;
        }
        press(was, false);
        if (key >= 0) {
            fingers.put(pointer, key);
        } else {
            fingers.delete(pointer);
        }
        press(key, true);
    }

    @Override
    public boolean onTouchEvent(MotionEvent e) {
        if (editing) {
            editTouch(e);
            return true;
        }
        int action = e.getActionMasked();
        switch (action) {
            case MotionEvent.ACTION_DOWN:
            case MotionEvent.ACTION_POINTER_DOWN: {
                int i = e.getActionIndex();
                int key = hit(e.getX(i), e.getY(i));
                if (key == SETTINGS) {
                    settingsFinger = e.getPointerId(i);
                }
                track(e.getPointerId(i), key);
                break;
            }
            case MotionEvent.ACTION_MOVE:
                for (int i = 0; i < e.getPointerCount(); i++) {
                    int id = e.getPointerId(i);
                    int was = fingers.get(id, -1);
                    float x = e.getX(i);
                    float y = e.getY(i);
                    int now = hit(x, y);
                    // A walking thumb keeps walking across the gaps and over
                    // 5 instead of stopping or firing it. Only the number
                    // block's own keys: one moved into it still fires.
                    if (walks(was) && !walks(now) && (now < 0 || inNumberBlock(now))
                            && numberBlock.contains(x, y)) {
                        now = was;
                    }
                    // A finger that slides off the keys entirely lets go.
                    track(id, now);
                }
                break;
            case MotionEvent.ACTION_UP:
            case MotionEvent.ACTION_POINTER_UP: {
                int id = e.getPointerId(e.getActionIndex());
                // ⚙ opens on the finger that came down on it, lifting from it;
                // one that only slid across it on the way somewhere does not.
                boolean open = id == settingsFinger && fingers.get(id, -1) == SETTINGS;
                if (id == settingsFinger) {
                    settingsFinger = -1;
                }
                track(id, -1);
                if (open) {
                    startEditing();
                }
                break;
            }
            case MotionEvent.ACTION_CANCEL:
                releaseAll();
                break;
            default:
                break;
        }
        return true;
    }

    /** Let go of every key — a cancel, a size change, focus going away. */
    void releaseAll() {
        for (int i = 0; i < fingers.size(); i++) {
            int key = fingers.valueAt(i);
            if (holds[key] > 0) {
                holds[key] = 1;
                press(key, false);
            }
        }
        fingers.clear();
        settingsFinger = -1;
        invalidate();
    }

    @Override
    public void onWindowFocusChanged(boolean hasFocus) {
        super.onWindowFocusChanged(hasFocus);
        if (!hasFocus) {
            releaseAll();
            if (editing && dragKey >= 0) {
                save();
            }
            endGesture();
        }
    }

    @Override
    protected void onDetachedFromWindow() {
        releaseAll();
        super.onDetachedFromWindow();
    }

    // ---- Editing ---------------------------------------------------------
    //
    // One finger on a key drags it. A second finger while the first holds a
    // key pinches it: the size follows the distance between the two, about the
    // key's own centre. Two fingers that land with the first on no key pinch
    // the key under the second, or under the point between them. Lifting
    // either finger of a pinch hands the key back to the other as a drag. A
    // tap on ⚙ ends the editing, as 완료 does; every gesture that ends saves.

    private void startEditing() {
        releaseAll();
        endGesture();
        editing = true;
        panel.setVisibility(VISIBLE);
        performHapticFeedback(HapticFeedbackConstants.LONG_PRESS);
        invalidate();
    }

    void stopEditing() {
        endGesture();
        editing = false;
        save();
        panel.setVisibility(GONE);
        invalidate();
    }

    void resetLayout() {
        endGesture();
        resetToGrid();
        fitAll();
        if (layoutFile.exists() && !layoutFile.delete()) {
            save();
        }
        performHapticFeedback(HapticFeedbackConstants.LONG_PRESS);
        invalidate();
    }

    private void endGesture() {
        dragKey = -1;
        dragPointer = -1;
        pinchPointer = -1;
        sparePointer = -1;
    }

    /** Bring key {@code i} to the top. */
    private void raise(int i) {
        int n = 0;
        while (order[n] != i) {
            n++;
        }
        System.arraycopy(order, n + 1, order, n, order.length - n - 1);
        order[order.length - 1] = i;
    }

    /** Hold {@link #dragKey} by the finger at index {@code p} where it is now. */
    private void grab(MotionEvent e, int p) {
        dragPointer = e.getPointerId(p);
        grabX = e.getX(p) - boxes[dragKey].centerX();
        grabY = e.getY(p) - boxes[dragKey].centerY();
    }

    private void startPinch(MotionEvent e, int a, int b) {
        pinchPointer = e.getPointerId(b);
        pinchDist = (float) Math.hypot(e.getX(a) - e.getX(b), e.getY(a) - e.getY(b));
        pinchScale = boxes[dragKey].height() / Math.max(1f, cellH[dragKey]);
        moved = true;
    }

    private void editTouch(MotionEvent e) {
        switch (e.getActionMasked()) {
            case MotionEvent.ACTION_DOWN:
            case MotionEvent.ACTION_POINTER_DOWN: {
                int p = e.getActionIndex();
                float x = e.getX(p);
                float y = e.getY(p);
                if (dragKey < 0) {
                    int key = hit(x, y);
                    int spare = sparePointer < 0 ? -1 : e.findPointerIndex(sparePointer);
                    if (key < 0 && spare >= 0) {
                        key = hit((x + e.getX(spare)) / 2f, (y + e.getY(spare)) / 2f);
                    }
                    if (key < 0) {
                        if (sparePointer < 0) {
                            sparePointer = e.getPointerId(p);
                        }
                        break;
                    }
                    dragKey = key;
                    raise(key);
                    grab(e, p);
                    downX = x;
                    downY = y;
                    moved = false;
                    if (spare >= 0) {
                        startPinch(e, p, spare);
                    }
                    sparePointer = -1;
                    invalidate();
                } else if (pinchPointer < 0) {
                    int a = e.findPointerIndex(dragPointer);
                    if (a >= 0) {
                        startPinch(e, a, p);
                        invalidate();
                    }
                }
                break;
            }
            case MotionEvent.ACTION_MOVE: {
                if (dragKey < 0) {
                    break;
                }
                int a = e.findPointerIndex(dragPointer);
                if (a < 0) {
                    break;
                }
                if (pinchPointer >= 0) {
                    int b = e.findPointerIndex(pinchPointer);
                    if (b < 0 || pinchDist <= 0f) {
                        break;
                    }
                    float d = (float) Math.hypot(e.getX(a) - e.getX(b), e.getY(a) - e.getY(b));
                    resize(dragKey, pinchScale * d / pinchDist);
                } else {
                    float x = e.getX(a);
                    float y = e.getY(a);
                    if (!moved && Math.hypot(x - downX, y - downY) > slop) {
                        moved = true;
                    }
                    if (moved) {
                        move(dragKey, x - grabX, y - grabY);
                    }
                }
                invalidate();
                break;
            }
            case MotionEvent.ACTION_POINTER_UP: {
                int p = e.getActionIndex();
                int id = e.getPointerId(p);
                if (id == sparePointer) {
                    sparePointer = -1;
                } else if (id == pinchPointer) {
                    pinchPointer = -1;
                    int a = e.findPointerIndex(dragPointer);
                    if (a >= 0) {
                        grab(e, a);
                    }
                    invalidate();
                } else if (id == dragPointer) {
                    if (pinchPointer >= 0) {
                        int b = e.findPointerIndex(pinchPointer);
                        pinchPointer = -1;
                        if (b >= 0) {
                            grab(e, b);
                        } else {
                            finishGesture();
                        }
                        invalidate();
                    } else {
                        finishGesture();
                    }
                }
                break;
            }
            case MotionEvent.ACTION_UP:
                finishGesture();
                break;
            case MotionEvent.ACTION_CANCEL:
                if (dragKey >= 0) {
                    save();
                }
                endGesture();
                invalidate();
                break;
            default:
                break;
        }
    }

    private void finishGesture() {
        int key = dragKey;
        boolean tap = !moved;
        endGesture();
        if (key < 0) {
            return;
        }
        if (key == SETTINGS && tap) {
            stopEditing();
            return;
        }
        save();
        invalidate();
    }

    private void move(int i, float x, float y) {
        int w = getWidth();
        int h = getHeight();
        RectF b = boxes[i];
        x = clamp(x, b.width() / 2f, w - b.width() / 2f);
        y = clamp(y, b.height() / 2f, h - b.height() / 2f);
        fx[i] = x / w;
        fy[i] = y / h;
        fitAll();
    }

    private void resize(int i, float s) {
        scale[i] = clamp(s, MIN_SCALE, maxScale(i));
        fit(i);
        // A key grown against an edge is pushed off it; keep where it went.
        fx[i] = boxes[i].centerX() / getWidth();
        fy[i] = boxes[i].centerY() / getHeight();
        fitAll();
    }

    /**
     * Covers the game picture while the keypad is being edited: says how,
     * offers 초기화 and 완료, and keeps the game from seeing those touches.
     */
    private static final class EditPanel extends View {
        private static final String[] HELP = {
            "버튼을 끌어서 옮깁니다.",
            "버튼을 짚은 채 다른 손가락으로",
            "벌리거나 오므리면 크기가 바뀝니다.",
            "완료나 설정 버튼을 누르면 끝납니다.",
        };

        private final PhoneKeypad pad;
        private final Paint paint = new Paint(Paint.ANTI_ALIAS_FLAG);
        private final RectF reset = new RectF();
        private final RectF done = new RectF();
        /** 0 = 초기화, 1 = 완료, -1 = neither, under the finger that came down. */
        private int pressed = -1;

        EditPanel(Activity activity, PhoneKeypad pad) {
            super(activity);
            this.pad = pad;
            setVisibility(GONE);
        }

        @Override
        protected void onSizeChanged(int w, int h, int ow, int oh) {
            float bw = w * 0.3f;
            float bh = Math.min(w * 0.13f, h * 0.1f);
            float gap = w * 0.06f;
            float y = h * 0.64f;
            reset.set(w / 2f - gap / 2f - bw, y, w / 2f - gap / 2f, y + bh);
            done.set(w / 2f + gap / 2f, y, w / 2f + gap / 2f + bw, y + bh);
        }

        @Override
        protected void onDraw(Canvas c) {
            int w = getWidth();
            int h = getHeight();
            c.drawColor(0xD8000000);
            paint.setTextAlign(Paint.Align.CENTER);
            paint.setStyle(Paint.Style.FILL);
            paint.setColor(0xFFD5B34B);
            paint.setTextSize(w / 12f);
            c.drawText("키패드 편집", w / 2f, h * 0.3f, paint);
            paint.setColor(0xFFA4A9B3);
            paint.setTextSize(w / 24f);
            float line = paint.getFontSpacing() * 1.25f;
            for (int i = 0; i < HELP.length; i++) {
                c.drawText(HELP[i], w / 2f, h * 0.4f + i * line, paint);
            }
            button(c, reset, "초기화", pressed == 0, false);
            button(c, done, "완료", pressed == 1, true);
        }

        private void button(Canvas c, RectF b, String label, boolean down, boolean main) {
            float r = b.height() / 4f;
            paint.setStyle(Paint.Style.FILL);
            paint.setColor(down ? 0xFF302814 : 0xFF080B10);
            c.drawRoundRect(b, r, r, paint);
            paint.setStyle(Paint.Style.STROKE);
            paint.setStrokeWidth(main ? 2.5f : 1.5f);
            paint.setColor(main || down ? 0xFFAA8734 : 0xFF657080);
            c.drawRoundRect(b, r, r, paint);
            paint.setStyle(Paint.Style.FILL);
            paint.setColor(main ? 0xFFD5B34B : 0xFFA4A9B3);
            paint.setTextSize(b.height() * 0.42f);
            float y = b.centerY() - (paint.ascent() + paint.descent()) / 2f;
            c.drawText(label, b.centerX(), y, paint);
        }

        private int which(float x, float y) {
            return reset.contains(x, y) ? 0 : done.contains(x, y) ? 1 : -1;
        }

        @Override
        public boolean onTouchEvent(MotionEvent e) {
            switch (e.getActionMasked()) {
                case MotionEvent.ACTION_DOWN:
                    pressed = which(e.getX(), e.getY());
                    invalidate();
                    break;
                case MotionEvent.ACTION_MOVE:
                    if (pressed >= 0 && which(e.getX(), e.getY()) != pressed) {
                        pressed = -1;
                        invalidate();
                    }
                    break;
                case MotionEvent.ACTION_UP: {
                    int was = pressed;
                    pressed = -1;
                    invalidate();
                    if (was == 0) {
                        pad.resetLayout();
                    } else if (was == 1) {
                        pad.stopEditing();
                    }
                    break;
                }
                case MotionEvent.ACTION_CANCEL:
                    pressed = -1;
                    invalidate();
                    break;
                default:
                    break;
            }
            // Everything over the game is the editor's while it is open.
            return true;
        }
    }
}
