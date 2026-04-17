/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 * 
 * Could not load the following classes:
 *  javax.media.opengl.GL
 */
import UWBGL_JavaOpenGL.UWBGL_DrawHelper;
import UWBGL_JavaOpenGL.UWBGL_ELevelOfDetail;
import UWBGL_JavaOpenGL.UWBGL_SceneNode;
import UWBGL_JavaOpenGL.math3d.Vec3;
import javax.media.opengl.GL;

public class Projectile
extends AbstractMunition {
    @Override
    public void draw(GL gl, UWBGL_ELevelOfDetail lod, UWBGL_DrawHelper helper) {
        throw new UnsupportedOperationException("Not supported yet.");
    }

    @Override
    public boolean intersects(UWBGL_SceneNode root, Entity other, UWBGL_DrawHelper helper) {
        throw new UnsupportedOperationException("Not supported yet.");
    }

    @Override
    public void setVisible(boolean v) {
        throw new UnsupportedOperationException("Not supported yet.");
    }

    @Override
    public Vec3 findIntersect(UWBGL_SceneNode root, Entity other, UWBGL_DrawHelper helper) {
        throw new UnsupportedOperationException("Not supported yet.");
    }

    @Override
    public void collidedAt(Vec3 intercept) {
        throw new UnsupportedOperationException("Not supported yet.");
    }

    @Override
    public Vec3 getFacingDirection() {
        throw new UnsupportedOperationException("Not supported yet.");
    }

    @Override
    public void continueFiring(Vec3 cur_pos, Vec3 cur_vel) {
        throw new UnsupportedOperationException("Not supported yet.");
    }

    @Override
    public void fire(Vec3 init_pos, Vec3 init_vel, Vec3 parent_vel) {
        throw new UnsupportedOperationException("Not supported yet.");
    }
}

