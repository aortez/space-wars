/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 * 
 * Could not load the following classes:
 *  javax.media.opengl.GL
 */
package UWBGL_JavaOpenGL.Primitives;

import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingLine;
import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingSphere;
import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingVolume;
import UWBGL_JavaOpenGL.Primitives.UWBGL_Primitive;
import UWBGL_JavaOpenGL.UWBGL_DrawHelper;
import UWBGL_JavaOpenGL.UWBGL_ELevelOfDetail;
import UWBGL_JavaOpenGL.math3d.Vec3;
import javax.media.opengl.GL;

public class UWBGL_PrimitiveLine
extends UWBGL_Primitive {
    private Vec3 m_end1;
    private Vec3 m_end2;

    public UWBGL_PrimitiveLine() {
        this(new Vec3(0.0f, 0.0f, 0.0f), new Vec3(0.0f, 0.0f, 0.0f));
        this.m_bBeingDefined = true;
    }

    public UWBGL_PrimitiveLine(Vec3 end1, Vec3 end2) {
        this.m_end1 = end1;
        this.m_end2 = end2;
        this.m_bBeingDefined = false;
    }

    @Override
    public float getSize() {
        return this.m_end2.distanceTo(this.m_end1);
    }

    @Override
    public void setSize(float size) {
        this.m_end2.minusEquals(this.m_end1).normalizedEquals().timesEquals(size).plusEquals(this.m_end1);
    }

    public void setCenter(Vec3 center) {
        this.moveTo(center);
    }

    public Vec3 getCenter() {
        return this.m_end1.midpoint(this.m_end2);
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

    @Override
    public Vec3 getLocation() {
        return this.getCenter();
    }

    public float getLength() {
        return this.m_end2.minus(this.m_end1).length();
    }

    @Override
    public UWBGL_BoundingVolume getBoundingVolume(UWBGL_ELevelOfDetail lod) {
        switch (lod) {
            case Low: {
                return new UWBGL_BoundingSphere(this.getCenter(), this.getLength() * 0.5f);
            }
        }
        return new UWBGL_BoundingLine(this.m_end1, this.m_end2);
    }

    @Override
    protected void drawPrimitive(GL gl, UWBGL_ELevelOfDetail lod, UWBGL_DrawHelper draw_helper) {
        UWBGL_ELevelOfDetail oldLod = draw_helper.getLOD();
        draw_helper.setLOD(lod);
        draw_helper.drawLine(gl, this.m_end1, this.m_end2);
        draw_helper.setLOD(oldLod);
    }

    @Override
    public UWBGL_Primitive clone() {
        UWBGL_PrimitiveLine other = new UWBGL_PrimitiveLine(this.m_end1, this.m_end2);
        other.setFillMode(this.m_FillMode);
        other.setShadeMode(this.m_ShadeMode);
        other.setShadingColor(this.m_ShadingColor);
        other.setFlatColor(this.m_FlatColor);
        other.m_bBeingDefined = this.m_bBeingDefined;
        return other;
    }

    @Override
    public void definitionAction(int definitionAction, float x, float y) {
        if (this.m_bBeingDefined) {
            if (definitionAction == 0) {
                this.m_end2 = this.m_end1 = new Vec3(x, y, 0.0f);
            } else if (definitionAction == 1) {
                this.m_end2 = new Vec3(x, y, 0.0f);
            } else if (definitionAction == 2) {
                this.m_bBeingDefined = false;
                this.m_end2 = new Vec3(x, y, 0.0f);
                Vec3 offset = this.m_end1.minus(this.m_end2);
                if (offset.length() < 1.0f) {
                    this.m_end2 = new Vec3(this.m_end1.x + 1.0f, this.m_end1.y + 1.0f, 0.0f);
                    offset = this.m_end1.minus(this.m_end2);
                }
            }
        }
    }

    @Override
    public float getArea() {
        return this.getLength();
    }
}

