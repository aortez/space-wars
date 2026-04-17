/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 */
import UWBGL_JavaOpenGL.Primitives.UWBGL_Primitive;
import UWBGL_JavaOpenGL.math3d.Vec3;

public class Shell
extends Debris {
    public Shell(Vec3 loc, UWBGL_Primitive shape, Vec3 velocity, float damage) {
        super(loc, shape, velocity, damage, false);
        this.setLife(1.0f);
        this.m_lifeMax = 1.0f;
    }
}

