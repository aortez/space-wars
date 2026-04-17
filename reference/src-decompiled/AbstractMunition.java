/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 * 
 * Could not load the following classes:
 *  javax.media.opengl.GL
 */
import UWBGL_JavaOpenGL.Primitives.UWBGL_PrimitiveList;
import UWBGL_JavaOpenGL.UWBGL_DrawHelper;
import UWBGL_JavaOpenGL.UWBGL_ELevelOfDetail;
import UWBGL_JavaOpenGL.math3d.Vec3;
import javax.media.opengl.GL;

public abstract class AbstractMunition
extends UWBGL_PrimitiveList
implements Munition {
    public Vec3 m_Velocity;
    public Vec3 m_Direction;
    public Vec3 m_Mass;
    protected boolean m_firing = false;
    protected float m_Damage = 0.0f;

    @Override
    public void draw(GL gl, UWBGL_ELevelOfDetail lod, UWBGL_DrawHelper helper) {
    }

    @Override
    public Debris update(float time_delta) {
        throw new UnsupportedOperationException("Not supported yet.");
    }

    @Override
    public void setVisible(boolean v) {
        throw new UnsupportedOperationException("Not supported yet.");
    }

    @Override
    public float damageAmount() {
        return this.m_Damage;
    }

    @Override
    public void quitFiring() {
        this.m_firing = false;
    }

    @Override
    public boolean firing() {
        return this.m_firing;
    }
}

