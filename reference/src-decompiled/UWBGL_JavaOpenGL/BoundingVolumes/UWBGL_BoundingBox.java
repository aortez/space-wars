/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 * 
 * Could not load the following classes:
 *  javax.media.opengl.GL
 */
package UWBGL_JavaOpenGL.BoundingVolumes;

import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingSphere;
import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingVolume;
import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_EVolumeType;
import UWBGL_JavaOpenGL.UWBGL_Color;
import UWBGL_JavaOpenGL.UWBGL_Common;
import UWBGL_JavaOpenGL.UWBGL_DrawHelper;
import UWBGL_JavaOpenGL.UWBGL_EFillMode;
import UWBGL_JavaOpenGL.UWBGL_EShadeMode;
import UWBGL_JavaOpenGL.math3d.Vec3;
import javax.media.opengl.GL;

public class UWBGL_BoundingBox
implements UWBGL_BoundingVolume {
    private Vec3 m_min = new Vec3(0.0f, 0.0f, 0.0f);
    private Vec3 m_max = new Vec3(0.0f, 0.0f, 0.0f);

    public UWBGL_BoundingBox() {
    }

    public UWBGL_BoundingBox(Vec3 corner1, Vec3 corner2) {
        this();
        this.setCorners(corner1, corner2);
    }

    public UWBGL_BoundingBox(Vec3 center, float width, float height, float depth) {
        this();
        this.m_min.x = center.x - width * 0.5f;
        this.m_min.y = center.y - height * 0.5f;
        this.m_min.z = center.z - depth * 0.5f;
        this.m_max.x = center.x + width * 0.5f;
        this.m_max.y = center.y + height * 0.5f;
        this.m_max.z = center.z + depth * 0.5f;
    }

    @Override
    public UWBGL_EVolumeType getType() {
        return UWBGL_EVolumeType.Box;
    }

    public void setCorners(Vec3 corner1, Vec3 corner2) {
        this.m_min.x = corner1.x < corner2.x ? corner1.x : corner2.x;
        this.m_min.y = corner1.y < corner2.y ? corner1.y : corner2.y;
        this.m_min.z = corner1.z < corner2.z ? corner1.z : corner2.z;
        this.m_max.x = corner1.x > corner2.x ? corner1.x : corner2.x;
        this.m_max.y = corner1.y > corner2.y ? corner1.y : corner2.y;
        this.m_max.z = corner1.z > corner2.z ? corner1.z : corner2.z;
    }

    public void moveCenterTo(Vec3 new_center) {
        Vec3 old_center = this.getCenter();
        Vec3 delta = new_center.minusEquals(old_center);
        this.m_min.plusEquals(delta);
        this.m_max.plusEquals(delta);
    }

    @Override
    public Vec3 isContainedBy(Vec3 min, Vec3 max) {
        Vec3 fudgeVector = new Vec3();
        Vec3 lowerBound = this.m_min;
        Vec3 upperBound = this.m_max;
        if (lowerBound.x < min.x) {
            fudgeVector.x = min.x - lowerBound.x;
        } else if (upperBound.x > max.x) {
            fudgeVector.x = max.x - upperBound.x;
        }
        if (lowerBound.y < min.y) {
            fudgeVector.y = min.y - lowerBound.y;
        } else if (upperBound.y > max.y) {
            fudgeVector.y = max.y - upperBound.y;
        }
        return fudgeVector;
    }

    @Override
    public Vec3 isContainedBy(Vec3 center, float radius) {
        float distance = center.distanceTo(this.m_min);
        if (distance > radius) {
            return center.minus(this.m_min).normalizedEquals().timesEquals(distance - radius);
        }
        distance = center.distanceTo(this.m_max);
        if (distance > radius) {
            return center.minus(this.m_max).normalizedEquals().timesEquals(distance - radius);
        }
        return new Vec3();
    }

    @Override
    public boolean intersects(UWBGL_BoundingVolume other) {
        UWBGL_EVolumeType vt = other.getType();
        if (UWBGL_EVolumeType.Box == vt) {
            UWBGL_BoundingBox pOtherBox = (UWBGL_BoundingBox)other;
            return UWBGL_Common.intersectBoxBox(this.m_min, this.m_max, pOtherBox.getMin(), pOtherBox.getMax());
        }
        if (UWBGL_EVolumeType.List == vt) {
            return other.intersects(this);
        }
        if (UWBGL_EVolumeType.Sphere == vt) {
            UWBGL_BoundingSphere sphere = (UWBGL_BoundingSphere)other;
            return UWBGL_Common.intersectSphereBox(sphere.getCenter(), sphere.getRadius(), this.m_min, this.m_max);
        }
        System.out.println("UWBGL_BoundingBox: unknown intersection type: " + (Object)((Object)this.getType()) + " to " + (Object)((Object)other.getType()));
        System.exit(-1);
        return false;
    }

    public Vec3 getMin() {
        return this.m_min;
    }

    public Vec3 getMax() {
        return this.m_max;
    }

    @Override
    public Vec3 getCenter() {
        return this.m_min.midpoint(this.m_max);
    }

    public float width() {
        return this.m_max.x - this.m_min.x;
    }

    public float height() {
        return this.m_max.y - this.m_min.y;
    }

    public float depth() {
        return this.m_max.z - this.m_min.z;
    }

    @Override
    public boolean containsPoint(Vec3 location) {
        return UWBGL_Common.containsPoint(this.m_min, this.m_max, location);
    }

    @Override
    public void draw(GL gl, UWBGL_DrawHelper pDrawHelper) {
        pDrawHelper.resetAttributes(gl);
        pDrawHelper.setColor1(UWBGL_Color.RED);
        pDrawHelper.setShadeMode(UWBGL_EShadeMode.Flat);
        pDrawHelper.setFillMode(UWBGL_EFillMode.Wireframe);
        pDrawHelper.drawRectangle(gl, this.m_min, this.m_max);
    }

    public void makeInvalid() {
        this.m_min = new Vec3(1.0f, 1.0f, 1.0f);
        this.m_max = new Vec3(-1.0f, -1.0f, -1.0f);
    }

    public boolean isValid() {
        return this.m_min.x <= this.m_max.x && this.m_min.y <= this.m_max.y && this.m_min.z <= this.m_max.z;
    }

    @Override
    public void add(UWBGL_BoundingVolume pBV) {
        UWBGL_EVolumeType vt;
        if (pBV != null && UWBGL_EVolumeType.Box == (vt = pBV.getType())) {
            UWBGL_BoundingBox pBox = (UWBGL_BoundingBox)pBV;
            this.add(pBox);
        }
    }

    public void add(UWBGL_BoundingBox box) {
        if (!box.isValid()) {
            return;
        }
        if (!this.isValid()) {
            this.m_min = box.m_min;
            this.m_max = box.m_max;
        } else {
            this.m_min.x = this.m_min.x < box.m_min.x ? this.m_min.x : box.m_min.x;
            this.m_min.y = this.m_min.y < box.m_min.y ? this.m_min.y : box.m_min.y;
            this.m_min.z = this.m_min.z < box.m_min.z ? this.m_min.z : box.m_min.z;
            this.m_max.x = this.m_max.x > box.m_max.x ? this.m_max.x : box.m_max.x;
            this.m_max.y = this.m_max.y > box.m_max.y ? this.m_max.y : box.m_max.y;
            this.m_max.z = this.m_max.z > box.m_max.z ? this.m_max.z : box.m_max.z;
        }
    }
}

