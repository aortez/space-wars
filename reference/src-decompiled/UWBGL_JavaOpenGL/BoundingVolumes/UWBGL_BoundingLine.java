/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 * 
 * Could not load the following classes:
 *  javax.media.opengl.GL
 */
package UWBGL_JavaOpenGL.BoundingVolumes;

import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingBox;
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

public class UWBGL_BoundingLine
implements UWBGL_BoundingVolume {
    private Vec3 m_head;
    private Vec3 m_tail;
    private Vec3 m_min = new Vec3();
    private Vec3 m_max = new Vec3();

    public UWBGL_BoundingLine(Vec3 head, Vec3 tail) {
        this.m_head = head;
        this.m_tail = tail;
        this.calcMinMax();
    }

    @Override
    public UWBGL_EVolumeType getType() {
        return UWBGL_EVolumeType.Line;
    }

    public void moveCenterTo(Vec3 new_center) {
        Vec3 center = this.getCenter();
        Vec3 offset = new_center.minus(center);
        this.m_head.plusEquals(offset);
        this.m_tail.plusEquals(offset);
        this.m_min.plusEquals(offset);
        this.m_max.plusEquals(offset);
    }

    @Override
    public boolean intersects(UWBGL_BoundingVolume other) {
        switch (other.getType()) {
            case Sphere: {
                UWBGL_BoundingSphere pOtherSphere = (UWBGL_BoundingSphere)other;
                return UWBGL_Common.intersectLineSphere(this.m_head, this.m_tail, pOtherSphere.getCenter(), pOtherSphere.getRadius());
            }
            case Box: {
                UWBGL_BoundingBox box = (UWBGL_BoundingBox)other;
                return UWBGL_Common.intersectLineBox(this.m_head, this.m_tail, box.getMin(), box.getMax());
            }
            case Line: {
                UWBGL_BoundingLine line = (UWBGL_BoundingLine)other;
                return UWBGL_Common.intersectLineLine(this.m_head, this.m_tail, line.m_head, line.m_tail);
            }
            case List: {
                return other.intersects(this);
            }
        }
        throw new UnsupportedOperationException("UWBGL_BoundingLine: attempting to intersect with unknown type: " + (Object)((Object)other.getType()));
    }

    @Override
    public Vec3 getCenter() {
        return this.m_head.plus(this.m_tail).timesEquals(0.5f);
    }

    public void setCenter(Vec3 c) {
        this.moveCenterTo(c);
    }

    @Override
    public boolean containsPoint(Vec3 location) {
        throw new UnsupportedOperationException("Not supported yet.");
    }

    @Override
    public void draw(GL gl, UWBGL_DrawHelper pDrawHelper) {
        pDrawHelper.resetAttributes(gl);
        pDrawHelper.setColor1(UWBGL_Color.RED);
        pDrawHelper.setShadeMode(UWBGL_EShadeMode.Flat);
        pDrawHelper.setFillMode(UWBGL_EFillMode.Wireframe);
        pDrawHelper.drawLine(gl, this.m_tail, this.m_head);
    }

    @Override
    public void add(UWBGL_BoundingVolume pBV) {
        throw new UnsupportedOperationException("Not supported yet.");
    }

    @Override
    public Vec3 isContainedBy(Vec3 min, Vec3 max) {
        throw new UnsupportedOperationException("Not supported yet.");
    }

    @Override
    public Vec3 isContainedBy(Vec3 center, float radius) {
        throw new UnsupportedOperationException("Not supported yet.");
    }

    public Vec3 getHead() {
        return this.m_head;
    }

    public Vec3 getTail() {
        return this.m_tail;
    }

    public float getLength() {
        return this.m_head.distanceTo(this.m_tail);
    }

    private void calcMinMax() {
        if (this.m_head.x < this.m_tail.x) {
            this.m_min.x = this.m_head.x;
            this.m_max.x = this.m_tail.x;
        } else {
            this.m_min.x = this.m_tail.x;
            this.m_max.x = this.m_head.x;
        }
        if (this.m_head.y < this.m_tail.y) {
            this.m_min.y = this.m_head.y;
            this.m_max.y = this.m_tail.y;
        } else {
            this.m_min.y = this.m_tail.y;
            this.m_max.y = this.m_head.y;
        }
    }
}

