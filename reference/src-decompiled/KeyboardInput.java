/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 */
import java.awt.event.KeyEvent;
import java.awt.event.KeyListener;

public class KeyboardInput
implements KeyListener {
    private static final int KEY_COUNT = 256;
    private boolean[] currentKeys = new boolean[256];
    private KeyState[] keys = new KeyState[256];

    public KeyboardInput() {
        this.clear();
    }

    public synchronized void poll() {
        for (int i = 0; i < 256; ++i) {
            if (this.currentKeys[i]) {
                if (this.keys[i] == KeyState.RELEASED) {
                    this.keys[i] = KeyState.ONCE;
                    continue;
                }
                this.keys[i] = KeyState.PRESSED;
                continue;
            }
            this.keys[i] = this.keys[i] == KeyState.RELEASED ? KeyState.UP : KeyState.RELEASED;
        }
    }

    public void clear() {
        for (int i = 0; i < 256; ++i) {
            this.currentKeys[i] = false;
            this.keys[i] = KeyState.RELEASED;
        }
    }

    public boolean keyDown(int keyCode) {
        return this.keys[keyCode] == KeyState.ONCE || this.keys[keyCode] == KeyState.PRESSED;
    }

    public boolean keyDownOnce(int keyCode) {
        return this.keys[keyCode] == KeyState.ONCE;
    }

    @Override
    public synchronized void keyPressed(KeyEvent e) {
        int keyCode = e.getKeyCode();
        if (keyCode >= 0 && keyCode < 256) {
            this.currentKeys[keyCode] = true;
        }
    }

    @Override
    public synchronized void keyReleased(KeyEvent e) {
        int keyCode = e.getKeyCode();
        if (keyCode >= 0 && keyCode < 256) {
            this.currentKeys[keyCode] = false;
        }
    }

    @Override
    public void keyTyped(KeyEvent e) {
    }

    public boolean wasReleased(int keyCode) {
        return this.keys[keyCode] == KeyState.RELEASED;
    }

    private static enum KeyState {
        UP,
        RELEASED,
        PRESSED,
        ONCE;

    }
}

