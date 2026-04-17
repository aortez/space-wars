/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 */
package UWBGL_JavaOpenGL;

import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingVolume;
import UWBGL_JavaOpenGL.Primitives.UWBGL_Primitive;
import UWBGL_JavaOpenGL.UWBGL_ELevelOfDetail;
import UWBGL_JavaOpenGL.math3d.Vec3;

public class UWBGL_BasicEntity {
    Vec3 m_Velocity;
    UWBGL_Primitive m_Primitive;
    protected boolean m_bCanMove;

    public UWBGL_BasicEntity(UWBGL_Primitive p, Vec3 velocity) {
        this.m_Velocity = velocity;
        this.m_Primitive = p;
        this.m_bCanMove = true;
    }

    public void update(float elapsed_seconds) {
        if (this.m_bCanMove) {
            Vec3 offset = this.m_Velocity.times(elapsed_seconds);
            this.m_Primitive.moveBy(offset);
        }
    }

    public boolean getCanMove() {
        return this.m_bCanMove;
    }

    public void setCanMove(boolean m) {
        this.m_bCanMove = m;
    }

    public boolean reboundWithin(Vec3 min, Vec3 max) {
        UWBGL_BoundingVolume bounds = this.m_Primitive.getBoundingVolume(UWBGL_ELevelOfDetail.High);
        Vec3 fudgeVector = bounds.isContainedBy(min, max);
        this.m_Primitive.moveBy(fudgeVector);
        if (fudgeVector.x != 0.0f) {
            this.m_Velocity = this.m_Velocity.times(new Vec3(-1.0f, 1.0f, 1.0f));
        }
        if (fudgeVector.y != 0.0f) {
            this.m_Velocity = this.m_Velocity.times(new Vec3(1.0f, -1.0f, 1.0f));
        }
        if (fudgeVector.z != 0.0f) {
            this.m_Velocity = this.m_Velocity.times(new Vec3(1.0f, 1.0f, -1.0f));
        }
        return fudgeVector.length() > 0.0f;
    }

    public void setVelocity(Vec3 velocity) {
        this.m_Velocity = velocity;
    }

    public Vec3 getVelocity() {
        return this.m_Velocity;
    }

    public boolean isStationary() {
        return 1.0E-9f >= Math.abs(this.m_Velocity.x) && 1.0E-9f >= Math.abs(this.m_Velocity.y) && 1.0E-9f >= Math.abs(this.m_Velocity.z);
    }

    public void collisionResponse(UWBGL_Primitive pOther, Vec3 other_location) {
        Vec3 dir = this.m_Primitive.getLocation().minus(other_location);
        dir = dir.normalized();
        float mySpeed = this.getVelocity().length();
        this.setVelocity(dir.times(mySpeed));
    }
}

