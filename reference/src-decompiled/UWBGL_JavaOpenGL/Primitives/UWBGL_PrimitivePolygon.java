/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 * 
 * Could not load the following classes:
 *  javax.media.opengl.GL
 */
package UWBGL_JavaOpenGL.Primitives;

import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingSphere;
import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingVolume;
import UWBGL_JavaOpenGL.Primitives.UWBGL_Primitive;
import UWBGL_JavaOpenGL.UWBGL_DrawHelper;
import UWBGL_JavaOpenGL.UWBGL_ELevelOfDetail;
import UWBGL_JavaOpenGL.Util.UWBGL_Util;
import UWBGL_JavaOpenGL.math3d.Vec3;
import javax.media.opengl.GL;

public class UWBGL_PrimitivePolygon
extends UWBGL_Primitive {
    private Vec3[] m_points;
    private int m_AdjustmentIndex;
    private int m_NextIndex;
    private int m_pointCount;
    private Vec3 m_center;
    private boolean m_dirty_center = true;
    private float m_area;
    private boolean m_dirty_area = true;

    public UWBGL_PrimitivePolygon(int pointCount) {
        this.m_pointCount = pointCount;
        this.m_points = new Vec3[this.m_pointCount];
        for (int i = 0; i < this.m_pointCount; ++i) {
            this.m_points[i] = new Vec3(0.0f, 0.0f, 0.0f);
        }
        this.m_bBeingDefined = true;
        this.m_AdjustmentIndex = 0;
        this.calcCenter();
    }

    public UWBGL_PrimitivePolygon(Vec3[] points) {
        this.m_pointCount = points.length;
        this.m_points = new Vec3[this.m_pointCount];
        for (int i = 0; i < this.m_pointCount; ++i) {
            this.m_points[i] = points[i].clone();
        }
        this.m_bBeingDefined = false;
        this.m_AdjustmentIndex = 0;
        this.calcCenter();
        this.calcArea();
    }

    public void setCenter(Vec3 center) {
        this.moveTo(center);
    }

    public Vec3 getCenter() {
        if (this.m_dirty_center) {
            this.calcCenter();
        }
        return this.m_center;
    }

    private void calcCenter() {
        Vec3 total = new Vec3(0.0f, 0.0f, 0.0f);
        for (int i = 0; i < this.m_pointCount; ++i) {
            total.plusEquals(this.m_points[i]);
        }
        this.m_center = total.divideEquals(this.m_pointCount);
        this.m_dirty_center = false;
    }

    @Override
    public void moveTo(Vec3 loc) {
        this.moveBy(loc.minus(this.getCenter()));
        this.m_dirty_center = true;
    }

    @Override
    public void moveBy(Vec3 offset) {
        for (int i = 0; i < this.m_pointCount; ++i) {
            this.m_points[i].plusEquals(offset);
        }
        this.m_dirty_center = true;
    }

    @Override
    public Vec3 getLocation() {
        return this.getCenter();
    }

    @Override
    public float getArea() {
        if (this.m_dirty_area) {
            this.calcArea();
        }
        return this.m_area;
    }

    private void calcArea() {
        if (this.m_pointCount < 3) {
            this.m_area = 1.0f;
            this.m_dirty_area = false;
            return;
        }
        Vec3 center = this.getCenter();
        float totalArea = 0.0f;
        for (int i = 0; i < this.m_pointCount - 1; ++i) {
            totalArea += UWBGL_Util.area(center, this.m_points[i], this.m_points[i + 1]);
        }
        this.m_area = totalArea += UWBGL_Util.area(center, this.m_points[0], this.m_points[this.m_pointCount - 1]);
        this.m_dirty_area = false;
    }

    @Override
    public float getSize() {
        return this.getArea();
    }

    @Override
    public void setSize(float size) {
        float area = this.getSize();
        float ratio = size / area;
        Vec3 center = this.getCenter();
        Vec3[] dir = new Vec3[this.m_pointCount];
        float[] newMagnitude = new float[this.m_pointCount];
        for (int i = 0; i < this.m_pointCount; ++i) {
            dir[i] = this.m_points[i].minus(center);
            newMagnitude[i] = dir[i].length() * ratio;
            dir[i] = dir[i].normalizedEquals().timesEquals(newMagnitude[i]);
            this.m_points[i] = dir[i].plus(center);
        }
        this.m_dirty_center = true;
        this.m_dirty_area = true;
    }

    @Override
    public UWBGL_BoundingVolume getBoundingVolume(UWBGL_ELevelOfDetail lod) {
        float newRadius = (float)Math.sqrt(this.getArea() * 0.99f / (float)Math.PI);
        return new UWBGL_BoundingSphere(this.getCenter().clone(), newRadius);
    }

    @Override
    protected void drawPrimitive(GL gl, UWBGL_ELevelOfDetail lod, UWBGL_DrawHelper draw_helper) {
        draw_helper.drawPolygon(gl, this.getCenter(), this.m_points);
    }

    @Override
    public void definitionAction(int definitionAction, float x, float y) {
        if (this.m_bBeingDefined) {
            if (definitionAction == 0) {
                this.m_AdjustmentIndex = 1;
                this.m_NextIndex = 2;
                this.m_points[0] = new Vec3(x, y, 0.0f);
                for (int i = 1; i < this.m_pointCount; ++i) {
                    this.m_points[i] = this.m_points[0];
                }
            } else if (definitionAction == 1) {
                this.m_points[this.m_AdjustmentIndex] = new Vec3(x, y, 0.0f);
            } else if (definitionAction == 2) {
                if (this.m_AdjustmentIndex < this.m_pointCount && this.m_NextIndex < this.m_pointCount) {
                    this.m_points[this.m_AdjustmentIndex] = new Vec3(x, y, 0.0f);
                    this.m_AdjustmentIndex = this.m_pointCount;
                } else if (this.m_NextIndex < this.m_pointCount) {
                    this.m_AdjustmentIndex = this.m_NextIndex++;
                    this.m_points[this.m_AdjustmentIndex] = new Vec3(x, y, 0.0f);
                } else {
                    this.m_bBeingDefined = false;
                    Vec3 offset = this.m_points[0].minus(new Vec3(x, y, 0.0f));
                }
            }
        }
        this.m_dirty_center = true;
        this.m_dirty_area = true;
    }

    @Override
    public UWBGL_Primitive clone() {
        UWBGL_PrimitivePolygon other = new UWBGL_PrimitivePolygon(this.m_points);
        other.setFillMode(this.m_FillMode);
        other.setShadeMode(this.m_ShadeMode);
        other.setShadingColor(this.m_ShadingColor);
        other.setFlatColor(this.m_FlatColor);
        other.m_bBeingDefined = this.m_bBeingDefined;
        return other;
    }
}

