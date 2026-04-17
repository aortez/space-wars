/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 * 
 * Could not load the following classes:
 *  javax.media.opengl.GL
 */
import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingLine;
import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingList;
import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingSphere;
import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingVolume;
import UWBGL_JavaOpenGL.UWBGL_Color;
import UWBGL_JavaOpenGL.UWBGL_Common;
import UWBGL_JavaOpenGL.UWBGL_DrawHelper;
import UWBGL_JavaOpenGL.UWBGL_ELevelOfDetail;
import UWBGL_JavaOpenGL.UWBGL_SceneNode;
import UWBGL_JavaOpenGL.math3d.Vec3;
import javax.media.opengl.GL;

public class Laser
extends AbstractMunition
implements Common,
CausesDamage {
    static final UWBGL_Color LASER_COLOR_1 = UWBGL_Color.scale255(255.0f, 0.0f, 0.0f);
    static final UWBGL_Color LASER_COLOR_2 = UWBGL_Color.scale255(150.0f, 0.0f, 0.0f);
    static final UWBGL_Color LASER_COLOR_3 = UWBGL_Color.scale255(75.0f, 0.0f, 0.0f);
    private Vec3 head;
    private Vec3 tail;
    private Entity parent_entity = null;
    private UWBGL_BoundingVolume m_bounds;
    private boolean bounds_dirty = true;

    public Laser(Entity parent) {
        this.parent_entity = parent;
        this.m_Damage = 10.0f;
    }

    @Override
    public void fire(Vec3 init_pos, Vec3 init_dir, Vec3 parent_vel) {
        if (!this.m_firing) {
            this.m_Direction = this.parent_entity.getFacingDirection();
            this.tail = this.head = init_pos;
            this.m_firing = true;
        }
    }

    @Override
    public void continueFiring(Vec3 cur_pos, Vec3 cur_dir) {
        float length = this.head.distanceTo(this.tail);
        this.m_Direction = cur_dir;
        this.head = cur_pos;
        this.tail = this.head.plus(cur_dir.times(length));
    }

    @Override
    public void draw(GL gl, UWBGL_ELevelOfDetail lod, UWBGL_DrawHelper helper) {
        if (this.m_firing) {
            helper.setColor1(LASER_COLOR_1);
            helper.setTexturing(gl, false);
            Vec3[] points = new Vec3[4];
            Vec3 normal = this.m_Direction.rotateR(1.5707964f);
            normal.setZ(0.8f);
            points[0] = new Vec3(this.head.minus(normal.times(0.45f)));
            points[1] = new Vec3(this.head.plus(normal.times(0.45f)));
            points[2] = new Vec3(this.tail.plus(normal.times(0.25f)));
            points[3] = new Vec3(this.tail.minus(normal.times(0.25f)));
            helper.drawPolygon(gl, this.head.midpoint(this.tail), points);
            helper.setColor1(LASER_COLOR_2);
            normal.setZ(0.81f);
            points[0] = new Vec3(this.head.minus(normal.times(0.75f)));
            points[1] = new Vec3(this.head.plus(normal.times(0.75f)));
            points[2] = new Vec3(this.tail.plus(normal.times(0.75f)));
            points[3] = new Vec3(this.tail.minus(normal.times(0.75f)));
            helper.drawPolygon(gl, this.head.midpoint(this.tail), points);
            normal.setZ(0.82f);
            helper.setColor1(LASER_COLOR_3);
            points[0] = new Vec3(this.head.minus(normal.times(1.0f)));
            points[1] = new Vec3(this.head.plus(normal.times(1.0f)));
            points[2] = new Vec3(this.tail.plus(normal.times(1.0f)));
            points[3] = new Vec3(this.tail.minus(normal.times(1.0f)));
            helper.drawPolygon(gl, this.head.midpoint(this.tail), points);
        }
    }

    @Override
    public Vec3 findIntersect(UWBGL_SceneNode root, Entity other, UWBGL_DrawHelper helper) {
        UWBGL_BoundingLine line = (UWBGL_BoundingLine)this.getBounds(UWBGL_ELevelOfDetail.Low, helper);
        UWBGL_BoundingVolume other_bounds = null;
        other_bounds = other.hasParent() ? root.getNodeBounds(UWBGL_ELevelOfDetail.High, (UWBGL_SceneNode)((Object)other), helper) : other.getBounds(UWBGL_ELevelOfDetail.High, helper);
        switch (other_bounds.getType()) {
            case Sphere: {
                UWBGL_BoundingSphere sphere = (UWBGL_BoundingSphere)other_bounds;
                return UWBGL_Common.findIntersectLineSphere(line.getHead(), line.getTail(), sphere.getCenter(), sphere.getRadius());
            }
            case List: {
                UWBGL_BoundingList list = (UWBGL_BoundingList)other_bounds;
                return UWBGL_Common.findIntersectLineList(line.getHead(), line.getTail(), list);
            }
        }
        System.err.println("Laser: findIntersect is not implemented between lines and " + (Object)((Object)other_bounds.getType()));
        System.exit(-1);
        return new Vec3(0.0f, 0.0f);
    }

    @Override
    public boolean intersects(UWBGL_SceneNode root, Entity other, UWBGL_DrawHelper helper) {
        UWBGL_BoundingVolume this_low = null;
        UWBGL_BoundingVolume that_low = null;
        this_low = this.getBounds(UWBGL_ELevelOfDetail.Low, helper);
        if (this_low.intersects(that_low = other.hasParent() ? root.getNodeBounds(UWBGL_ELevelOfDetail.Low, (UWBGL_SceneNode)((Object)other), helper) : other.getBounds(UWBGL_ELevelOfDetail.Low, helper))) {
            UWBGL_BoundingVolume that_high = null;
            that_high = other.hasParent() ? root.getNodeBounds(UWBGL_ELevelOfDetail.High, (UWBGL_SceneNode)((Object)other), helper) : other.getBounds(UWBGL_ELevelOfDetail.High, helper);
            return this_low.intersects(that_high);
        }
        return false;
    }

    @Override
    public Debris update(float time_delta) {
        if (this.m_firing) {
            this.head = ((Mount)((Object)this.parent_entity)).getMountCenter();
            float length = this.head.distanceTo(this.tail) + 50.0f;
            this.tail = this.head.plus(this.m_Direction.times(length));
            this.dirty();
        }
        return null;
    }

    @Override
    public void setVisible(boolean v) {
        this.m_bVisible = true;
    }

    public void dirty() {
        this.bounds_dirty = true;
    }

    public UWBGL_BoundingVolume getBounds(UWBGL_ELevelOfDetail lod, UWBGL_DrawHelper helper) {
        if (this.bounds_dirty) {
            this.calcBounds();
        }
        return this.m_bounds;
    }

    private void calcBounds() {
        this.m_bounds = new UWBGL_BoundingLine(this.head, this.tail);
        this.bounds_dirty = false;
    }

    @Override
    public void collidedAt(Vec3 intercept) {
        this.tail = intercept;
        this.dirty();
    }

    public Vec3 getHead() {
        return this.head;
    }

    public Vec3 getTail() {
        return this.tail;
    }

    @Override
    public Vec3 getFacingDirection() {
        return this.m_Direction;
    }

    public String toString() {
        return "head: " + this.head + "tail: " + this.tail + " firing: " + this.firing();
    }

    public float length() {
        return this.head.distanceTo(this.tail);
    }

    @Override
    public float damageAmount(Vec3 relative_velocity) {
        return 10.0f / this.length();
    }
}

