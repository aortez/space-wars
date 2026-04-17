/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 */
import UWBGL_JavaOpenGL.math3d.Vec3;

public interface HasLife {
    public float getLife();

    public void setLife(float var1);

    public void translateLife(float var1);

    public boolean dead();

    public Vec3 getVelocity();
}

