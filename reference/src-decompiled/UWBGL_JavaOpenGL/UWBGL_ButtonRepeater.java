/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 */
package UWBGL_JavaOpenGL;

import UWBGL_JavaOpenGL.Util.UWBGL_Timer;
import UWBGL_JavaOpenGL.Util.UWBGL_TimerListener;
import java.awt.event.ActionEvent;
import java.awt.event.ActionListener;
import javax.swing.JButton;

class UWBGL_ButtonRepeater
implements UWBGL_TimerListener {
    private JButton m_CurrentButton;
    private UWBGL_Timer m_Timer = new UWBGL_Timer(100L);
    public static final long DEFAULT_SLEEP_TIME = 100L;

    public UWBGL_ButtonRepeater() {
        this.m_Timer.addTimerListener(this);
    }

    public void setButton(JButton b) {
        this.m_CurrentButton = b;
    }

    public void setWaitTime(long waitTime) {
        this.m_Timer.setSleepTime(waitTime);
    }

    public void start() {
        this.m_Timer.start();
    }

    public void stop() {
        this.m_Timer.stop();
    }

    @Override
    public void timerEvent() {
        ActionListener[] listeners;
        if (this.m_CurrentButton == null) {
            return;
        }
        for (ActionListener l : listeners = this.m_CurrentButton.getActionListeners()) {
            l.actionPerformed(new ActionEvent(this, 1, "ButtonRepeated"));
        }
    }
}

