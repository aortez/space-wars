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
import UWBGL_JavaOpenGL.math3d.Vec3;
import javax.media.opengl.GL;

public class UWBGL_PrimitiveRectangle
extends UWBGL_Primitive {
    private Vec3 m_end1;
    private Vec3 m_end2;

    public UWBGL_PrimitiveRectangle() {
        this(new Vec3(), new Vec3());
        this.m_bBeingDefined = true;
    }

    public UWBGL_PrimitiveRectangle(Vec3 end1, Vec3 end2) {
        this.m_end1 = end1;
        this.m_end2 = end2;
        this.m_bBeingDefined = false;
        this.sortPoints();
    }

    @Override
    public float getSize() {
        return this.m_end2.distanceTo(this.m_end1);
    }

    @Override
    public void setSize(float size) {
        this.m_end2.minusEquals(this.m_end1).normalizedEquals().timesEquals(size).plusEquals(this.m_end1);
        this.sortPoints();
    }

    public void setMin(Vec3 min) {
        this.m_end1 = min;
        this.sortPoints();
    }

    public void setMax(Vec3 max) {
        this.m_end2 = max;
        this.sortPoints();
    }

    public Vec3 getMin() {
        return this.m_end1;
    }

    public Vec3 getMax() {
        return this.m_end2;
    }

    @Override
    public void moveTo(Vec3 loc) {
        this.moveBy(loc.minus(this.getCenter()));
    }

    @Override
    public void moveBy(Vec3 offset) {
        this.m_end1.plusEquals(offset);
        this.m_end2.plusEquals(offset);
    }

    public Vec3 getCenter() {
        return this.m_end1.midpoint(this.m_end2);
    }

    public void setSize(float width, float height) {
        this.m_end2.x = this.m_end1.x + width;
        this.m_end2.y = this.m_end1.y + height;
        this.sortPoints();
    }

    @Override
    public Vec3 getLocation() {
        return this.getCenter();
    }

    public float getWidth() {
        return Math.abs(this.m_end2.x - this.m_end1.x);
    }

    public float getHeight() {
        return Math.abs(this.m_end2.y - this.m_end1.y);
    }

    @Override
    public UWBGL_BoundingVolume getBoundingVolume(UWBGL_ELevelOfDetail lod) {
        Vec3 center = this.getCenter();
        float width = this.getWidth();
        float height = this.getHeight();
        if (lod == UWBGL_ELevelOfDetail.Low) {
            return new UWBGL_BoundingSphere(center, center.distanceTo(this.m_end1) * 0.99f);
        }
        UWBGL_BoundingList boundsList = new UWBGL_BoundingList();
        float radius = Math.min(width, height) * 0.99f / 2.0f;
        float cornerRadius = radius / 4.0f;
        Vec3 corner1 = new Vec3(this.m_end1.x + cornerRadius, this.m_end1.y + cornerRadius);
        Vec3 corner3 = new Vec3(this.m_end2.x - cornerRadius, this.m_end2.y - cornerRadius);
        Vec3 corner2 = new Vec3(corner1.x, corner3.y, 0.0f);
        Vec3 corner4 = new Vec3(corner3.x, corner1.y, 0.0f);
        boundsList.add(new UWBGL_BoundingSphere(corner1, cornerRadius));
        boundsList.add(new UWBGL_BoundingSphere(corner2, cornerRadius));
        boundsList.add(new UWBGL_BoundingSphere(corner3, cornerRadius));
        boundsList.add(new UWBGL_BoundingSphere(corner4, cornerRadius));
        boundsList.add(new UWBGL_BoundingSphere(center, radius));
        if (width > height) {
            float offset;
            for (offset = radius; offset <= width / 2.0f - radius; offset += radius) {
                boundsList.add(new UWBGL_BoundingSphere(new Vec3(offset + center.x, center.y, center.z), radius));
                boundsList.add(new UWBGL_BoundingSphere(new Vec3(center.x - offset, center.y, center.z), radius));
            }
            if (offset < width * 0.5f) {
                boundsList.add(new UWBGL_BoundingSphere(new Vec3(center.x + width * 0.5f - radius, center.y, center.z), radius));
                boundsList.add(new UWBGL_BoundingSphere(new Vec3(center.x - width * 0.5f + radius, center.y, center.z), radius));
            }
        } else {
            float offset;
            for (offset = radius; offset <= height / 2.0f - radius; offset += radius) {
                boundsList.add(new UWBGL_BoundingSphere(new Vec3(center.x, center.y + offset, center.z), radius));
                boundsList.add(new UWBGL_BoundingSphere(new Vec3(center.x, center.y - offset, center.z), radius));
            }
            if (offset < height * 0.5f) {
                boundsList.add(new UWBGL_BoundingSphere(new Vec3(center.x, center.y + height * 0.5f - radius, center.z), radius));
                boundsList.add(new UWBGL_BoundingSphere(new Vec3(center.x, center.y - height * 0.5f + radius, center.z), radius));
            }
        }
        return boundsList;
    }

    @Override
    protected void drawPrimitive(GL gl, UWBGL_ELevelOfDetail lod, UWBGL_DrawHelper draw_helper) {
        UWBGL_ELevelOfDetail oldLod = draw_helper.getLOD();
        draw_helper.setLOD(lod);
        draw_helper.drawRectangle(gl, this.m_end1, this.m_end2);
        draw_helper.setLOD(oldLod);
    }

    protected void sortPoints() {
        if (this.m_end1.x <= this.m_end2.x && this.m_end1.y <= this.m_end2.y && this.m_end1.z <= this.m_end2.z) {
            return;
        }
        Vec3 bottomLeft = new Vec3();
        bottomLeft.x = Math.min(this.m_end1.x, this.m_end2.x);
        bottomLeft.y = Math.min(this.m_end1.y, this.m_end2.y);
        bottomLeft.z = Math.min(this.m_end1.z, this.m_end2.z);
        Vec3 topRight = new Vec3();
        topRight.x = Math.max(this.m_end1.x, this.m_end2.x);
        topRight.y = Math.max(this.m_end1.y, this.m_end2.y);
        topRight.z = Math.max(this.m_end1.z, this.m_end2.z);
        this.m_end1 = bottomLeft;
        this.m_end2 = topRight;
    }

    @Override
    public void definitionAction(int definitionAction, float x, float y) {
        if (this.m_bBeingDefined) {
            if (definitionAction == 0) {
                this.m_end1.set(x, y, 0.0f);
                this.m_end2 = this.m_end1.clone();
            } else if (definitionAction == 1) {
                this.m_end2.set(x, y, 0.0f);
            } else if (definitionAction == 2) {
                this.m_bBeingDefined = false;
                this.m_end2.set(x, y, 0.0f);
                Vec3 offset = this.m_end1.minus(this.m_end2);
                if (offset.length() < 1.0f) {
                    this.m_end2.set(this.m_end1.x + 1.0f, this.m_end1.y + 1.0f, 0.0f);
                    offset = this.m_end1.minus(this.m_end2);
                }
                this.sortPoints();
            }
        }
    }

    @Override
    public UWBGL_Primitive clone() {
        UWBGL_PrimitiveRectangle other = new UWBGL_PrimitiveRectangle(this.m_end1.clone(), this.m_end2.clone());
        other.setFillMode(this.m_FillMode);
        other.setShadeMode(this.m_ShadeMode);
        other.setShadingColor(this.m_ShadingColor);
        other.setFlatColor(this.m_FlatColor);
        other.m_bBeingDefined = this.m_bBeingDefined;
        return other;
    }

    @Override
    public float getArea() {
        return this.getWidth() * this.getHeight();
    }
}

