/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 * 
 * Could not load the following classes:
 *  javax.media.opengl.GL
 */
package UWBGL_JavaOpenGL.Primitives;

import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingList;
import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingSphere;
import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingVolume;
import UWBGL_JavaOpenGL.Primitives.UWBGL_Primitive;
import UWBGL_JavaOpenGL.UWBGL_DrawHelper;
import UWBGL_JavaOpenGL.UWBGL_ELevelOfDetail;
import UWBGL_JavaOpenGL.Util.UWBGL_Util;
import UWBGL_JavaOpenGL.math3d.Vec3;
import javax.media.opengl.GL;

public class UWBGL_PrimitiveTriangle
extends UWBGL_Primitive {
    private Vec3[] m_points = new Vec3[3];
    private int m_AdjustmentIndex;
    private static final float cornerRadius = 0.1f;

    public UWBGL_PrimitiveTriangle() {
        this(new Vec3(0.0f, 0.0f, 0.0f), new Vec3(0.0f, 0.0f, 0.0f), new Vec3(0.0f, 0.0f, 0.0f));
        this.m_bBeingDefined = true;
    }

    public UWBGL_PrimitiveTriangle(Vec3 point1, Vec3 point2, Vec3 point3) {
        this.m_points[0] = point1;
        this.m_points[1] = point2;
        this.m_points[2] = point3;
        this.m_bBeingDefined = false;
        this.m_AdjustmentIndex = 0;
    }

    @Override
    public float getSize() {
        return UWBGL_Util.area(this.m_points[0], this.m_points[1], this.m_points[2]);
    }

    @Override
    public float getArea() {
        return UWBGL_Util.area(this.m_points[0], this.m_points[1], this.m_points[2]);
    }

    @Override
    public void setSize(float size) {
        float area = this.getSize();
        float ratio = size / area;
        Vec3 center = this.getCenter();
        Vec3[] dir = new Vec3[3];
        float[] newMagnitude = new float[3];
        for (int i = 0; i < 3; ++i) {
            dir[i] = this.m_points[i].minus(center);
            newMagnitude[i] = dir[i].length() * ratio;
            dir[i].normalizedEquals().timesEquals(newMagnitude[i]);
            this.m_points[i] = dir[i].plus(center);
        }
    }

    public void setCenter(Vec3 center) {
        this.moveTo(center);
    }

    public Vec3 getCenter() {
        return this.m_points[0].midpoint(this.m_points[1], this.m_points[2]);
    }

    @Override
    public void moveTo(Vec3 loc) {
        this.moveBy(loc.minus(this.getCenter()));
    }

    @Override
    public void moveBy(Vec3 offset) {
        for (int i = 0; i < 3; ++i) {
            this.m_points[i].plusEquals(offset);
        }
    }

    @Override
    public Vec3 getLocation() {
        return this.getCenter();
    }

    @Override
    public UWBGL_BoundingVolume getBoundingVolume(UWBGL_ELevelOfDetail lod) {
        if (lod == UWBGL_ELevelOfDetail.Low) {
            Vec3 center = this.getCenter();
            float radius = 1.0f;
            for (int i = 0; i < this.m_points.length; ++i) {
                radius = Math.max(radius, this.m_points[i].distanceTo(center));
            }
            return new UWBGL_BoundingSphere(center, radius * 0.99f);
        }
        UWBGL_BoundingList list = new UWBGL_BoundingList();
        Vec3 center = this.getCenter();
        for (int i = 0; i < this.m_points.length; ++i) {
            Vec3 corner = this.m_points[i].plus(center.minus(this.m_points[i]).normalizedEquals().timesEquals(0.1f));
            list.add(new UWBGL_BoundingSphere(corner, 0.1f));
        }
        Vec3 mid_side1 = this.m_points[0].midpoint(this.m_points[1]);
        Vec3 mid_side2 = this.m_points[0].midpoint(this.m_points[2]);
        Vec3 mid_side3 = this.m_points[1].midpoint(this.m_points[2]);
        float avg_distance = 0.333333f * (mid_side1.distanceTo(center) + mid_side2.distanceTo(center) + mid_side3.distanceTo(center));
        float min_size = avg_distance * 0.15f;
        if (min_size < 2.0f) {
            min_size = 2.0f;
        }
        this.getTriangleBoundsRecursive(this.m_points[0], this.m_points[1], this.m_points[2], list, min_size);
        return list;
    }

    @Override
    protected void drawPrimitive(GL gl, UWBGL_ELevelOfDetail lod, UWBGL_DrawHelper draw_helper) {
        UWBGL_ELevelOfDetail oldLod = draw_helper.getLOD();
        draw_helper.setLOD(lod);
        draw_helper.drawTriangle(gl, this.m_points);
        draw_helper.setLOD(oldLod);
    }

    @Override
    public void definitionAction(int definitionAction, float x, float y) {
        if (this.m_bBeingDefined) {
            if (definitionAction == 0) {
                this.m_AdjustmentIndex = 1;
                this.m_points[0] = new Vec3(x, y, 0.0f);
                this.m_points[1] = new Vec3(this.m_points[0]);
                this.m_points[2] = new Vec3(this.m_points[0]);
            } else if (definitionAction == 1) {
                this.m_points[this.m_AdjustmentIndex].set(x, y, 0.0f);
            } else if (definitionAction == 2) {
                if (this.m_AdjustmentIndex == 1) {
                    this.m_AdjustmentIndex = 3;
                } else if (this.m_AdjustmentIndex == 3) {
                    this.m_AdjustmentIndex = 2;
                    this.m_points[this.m_AdjustmentIndex] = new Vec3(x, y, 0.0f);
                } else if (this.m_AdjustmentIndex == 2) {
                    this.m_bBeingDefined = false;
                }
            }
        }
    }

    @Override
    public UWBGL_Primitive clone() {
        UWBGL_PrimitiveTriangle other = new UWBGL_PrimitiveTriangle(this.m_points[0].clone(), this.m_points[1].clone(), this.m_points[2].clone());
        other.setFillMode(this.m_FillMode);
        other.setShadeMode(this.m_ShadeMode);
        other.setShadingColor(this.m_ShadingColor);
        other.setFlatColor(this.m_FlatColor);
        other.m_bBeingDefined = this.m_bBeingDefined;
        return other;
    }

    private void getTriangleBoundsRecursive(Vec3 v1, Vec3 v2, Vec3 v3, UWBGL_BoundingList circle_list, float min_circle_size) {
        Vec3 center = v1.midpoint(v2, v3);
        Vec3 p1 = new Vec3(Vec3.closestPointOnSegment(center, v1, v2));
        Vec3 p2 = new Vec3(Vec3.closestPointOnSegment(center, v2, v3));
        Vec3 p3 = new Vec3(Vec3.closestPointOnSegment(center, v1, v3));
        float d1 = center.distanceTo(p1);
        float d2 = center.distanceTo(p2);
        float d3 = center.distanceTo(p3);
        UWBGL_BoundingSphere c = null;
        c = d1 < d2 && d1 < d3 ? new UWBGL_BoundingSphere(center, d1) : (d2 < d1 && d2 < d3 ? new UWBGL_BoundingSphere(center, d2) : new UWBGL_BoundingSphere(center, d3));
        if (!this.circleInsideCircleList(c, circle_list)) {
            circle_list.add(c);
        }
        Vec3 mid_side1 = v1.midpoint(v2);
        Vec3 mid_side2 = v2.midpoint(v3);
        Vec3 mid_side3 = v1.midpoint(v3);
        if (v1.distanceTo(center) > min_circle_size) {
            this.getTriangleBoundsRecursive(v1, mid_side1, mid_side3, circle_list, min_circle_size);
        }
        if (v2.distanceTo(center) > min_circle_size) {
            this.getTriangleBoundsRecursive(v2, mid_side1, mid_side2, circle_list, min_circle_size);
        }
        if (v3.distanceTo(center) > min_circle_size) {
            this.getTriangleBoundsRecursive(v3, mid_side2, mid_side3, circle_list, min_circle_size);
        }
        if (c.getRadius() > min_circle_size) {
            this.getTriangleBoundsRecursive(mid_side1, mid_side2, mid_side3, circle_list, min_circle_size);
        }
    }

    boolean circleInsideCircleList(UWBGL_BoundingSphere circ, UWBGL_BoundingList circs) {
        for (int i = 0; i < circs.size(); ++i) {
            UWBGL_BoundingSphere c = (UWBGL_BoundingSphere)circs.get(i);
            float d = circ.getCenter().distanceTo(c.getCenter());
            if (!(d < c.getRadius())) continue;
            return true;
        }
        return false;
    }
}

