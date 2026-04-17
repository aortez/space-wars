/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 */
package UWBGL_JavaOpenGL.Util;

import UWBGL_JavaOpenGL.Util.UWBGL_TimerListener;
import java.util.ArrayList;

public class UWBGL_Timer
implements Runnable {
    private boolean m_Running = false;
    private long m_SleepMiliSecs = 1000L;
    private ArrayList<UWBGL_TimerListener> m_Listeners = new ArrayList();
    private Thread m_TimerThread;
    public static final long DEFAULT_SLEEP_MILISECS = 1000L;

    public UWBGL_Timer() {
    }

    public UWBGL_Timer(long sleepMiliSecs) {
        this();
        this.m_SleepMiliSecs = sleepMiliSecs;
    }

    public UWBGL_Timer(long sleepMiliSecs, UWBGL_TimerListener listener) {
        this(sleepMiliSecs);
        this.addTimerListener(listener);
        this.start();
    }

    public void setSleepTime(long sleepMiliSecs) {
        this.m_SleepMiliSecs = sleepMiliSecs;
    }

    public long getSleepTime() {
        return this.m_SleepMiliSecs;
    }

    public void start() {
        if (!this.m_Running) {
            this.m_Running = true;
            this.m_TimerThread = new Thread(this);
            this.m_TimerThread.start();
        }
    }

    public void stop() {
        this.m_Running = false;
    }

    @Override
    public void run() {
        while (this.m_Running) {
            try {
                Thread.sleep(this.m_SleepMiliSecs);
                for (UWBGL_TimerListener listener : this.m_Listeners) {
                    listener.timerEvent();
                }
            } catch (InterruptedException e) {
                e.printStackTrace(System.out);
                this.m_Running = false;
            }
        }
    }

    public void addTimerListener(UWBGL_TimerListener listener) {
        this.m_Listeners.add(listener);
    }

    public void removeTimerListener(UWBGL_TimerListener listener) {
        this.m_Listeners.remove(listener);
    }
}

