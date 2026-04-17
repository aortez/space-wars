/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 */
package UWBGL_JavaOpenGL.Util;

public class UWBGL_Clock {
    private long m_PrevClockTime = System.currentTimeMillis();

    public float getSecondsElapsed() {
        long currentTime = System.currentTimeMillis();
        float timeLapse = currentTime - this.m_PrevClockTime;
        this.m_PrevClockTime = currentTime;
        return timeLapse / 1000.0f;
    }
}

