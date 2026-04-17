/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 */
import UWBGL_JavaOpenGL.Primitives.UWBGL_Primitive;
import UWBGL_JavaOpenGL.Primitives.UWBGL_PrimitiveTriangle;
import UWBGL_JavaOpenGL.UWBGL_DrawHelper;
import UWBGL_JavaOpenGL.UWBGL_SceneNode;
import UWBGL_JavaOpenGL.math3d.Vec3;

class Cannon
extends AbstractMunition
implements Common {
    private float remaining_cool_down_time = 0.0f;
    private Entity parent_entity = null;
    Shell newly_fired_shell = null;

    public Cannon(Entity parent) {
        this.parent_entity = parent;
        this.m_firing = false;
    }

    @Override
    public Vec3 findIntersect(UWBGL_SceneNode root, Entity other, UWBGL_DrawHelper helper) {
        return new Vec3();
    }

    @Override
    public void fire(Vec3 init_pos, Vec3 init_dir, Vec3 parent_vel) {
        if (this.remaining_cool_down_time <= 0.0f) {
            this.m_Direction = this.parent_entity.getFacingDirection();
            float theta = -this.m_Direction.angleRadians();
            Vec3 p1 = Vec3.normFromRadians(theta).timesEquals(-2.0f);
            Vec3 p2 = Vec3.normFromRadians(theta + 2.0943952f).timesEquals(-2.0f);
            Vec3 p3 = Vec3.normFromRadians(theta - 2.0943952f).timesEquals(-2.0f);
            UWBGL_PrimitiveTriangle shell_body = new UWBGL_PrimitiveTriangle(p1, p2, p3);
            Vec3 v = init_dir.times(300.0f).plusEquals(parent_vel);
            this.newly_fired_shell = new Shell(init_pos.plus(init_dir.times(5.0f)), (UWBGL_Primitive)shell_body, v, 0.1f);
            this.newly_fired_shell.setOmega(2.0f);
            this.remaining_cool_down_time = 0.5f;
            this.m_firing = true;
            this.parent_entity.translateVelocity(init_dir.times(-200.0f));
        }
    }

    @Override
    public void continueFiring(Vec3 cur_pos, Vec3 cur_vel) {
    }

    @Override
    public void quitFiring() {
    }

    @Override
    public boolean firing() {
        return this.m_firing;
    }

    @Override
    public void collidedAt(Vec3 intercept) {
    }

    @Override
    public boolean intersects(UWBGL_SceneNode root, Entity other, UWBGL_DrawHelper helper) {
        return false;
    }

    @Override
    public Vec3 getFacingDirection() {
        return this.m_Direction;
    }

    @Override
    public Debris update(float delta_t) {
        this.remaining_cool_down_time -= delta_t;
        Shell Shell2 = this.newly_fired_shell;
        this.newly_fired_shell = null;
        return Shell2;
    }

    private static enum State {
        firing,
        none;

    }
}

