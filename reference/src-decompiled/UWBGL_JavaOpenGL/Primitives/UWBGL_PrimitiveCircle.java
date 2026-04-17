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
import UWBGL_JavaOpenGL.math3d.Vec3;
import javax.media.opengl.GL;

public class UWBGL_PrimitiveCircle
extends UWBGL_Primitive {
    protected Vec3 m_center;
    protected float m_radius;

    public UWBGL_PrimitiveCircle() {
        this(new Vec3(0.0f, 0.0f, 0.0f), 0.0f);
        this.m_bBeingDefined = true;
    }

    public UWBGL_PrimitiveCircle(Vec3 center, float radius) {
        this.m_center = center;
        this.m_radius = radius;
        this.m_bBeingDefined = false;
    }

    @Override
    public float getArea() {
        return (float)(Math.PI * (double)this.m_radius * (double)this.m_radius);
    }

    @Override
    public float getSize() {
        return this.m_radius;
    }

    @Override
    public void setSize(float size) {
        this.m_radius = size;
    }

    public void setCenter(Vec3 center) {
        this.m_center = center;
    }

    public Vec3 getCenter() {
        return this.m_center;
    }

    @Override
    public Vec3 getLocation() {
        return this.m_center;
    }

    @Override
    public void moveTo(Vec3 loc) {
        this.setCenter(loc);
    }

    public void setRadius(float radius) {
        this.m_radius = radius;
    }

    @Override
    public void definitionAction(int definitionAction, float x, float y) {
        if (this.m_bBeingDefined) {
            if (definitionAction == 0) {
                this.setCenter(new Vec3(x, y, 0.0f));
                this.m_radius = 1.0f;
            } else if (definitionAction == 1) {
                Vec3 offset = this.m_center.minus(new Vec3(x, y, 0.0f));
                this.m_radius = offset.length();
            } else if (definitionAction == 2) {
                this.m_bBeingDefined = false;
                Vec3 offset = this.m_center.minus(new Vec3(x, y, 0.0f));
                this.m_radius = offset.length();
                if (this.m_radius < 1.0f) {
                    this.m_radius = 1.0f;
                }
            }
        }
    }

    @Override
    public UWBGL_BoundingVolume getBoundingVolume(UWBGL_ELevelOfDetail lod) {
        return new UWBGL_BoundingSphere(this.m_center.clone(), 0.99f * this.m_radius);
    }

    public float getRadius() {
        return this.m_radius;
    }

    @Override
    protected void drawPrimitive(GL gl, UWBGL_ELevelOfDetail lod, UWBGL_DrawHelper draw_helper) {
        draw_helper.drawCircle(gl, this.m_center, this.m_radius);
    }

    @Override
    public UWBGL_Primitive clone() {
        UWBGL_PrimitiveCircle other = new UWBGL_PrimitiveCircle(this.m_center, this.m_radius);
        other.setFillMode(this.m_FillMode);
        other.setShadeMode(this.m_ShadeMode);
        other.setShadingColor(this.m_ShadingColor);
        other.setFlatColor(this.m_FlatColor);
        other.m_bBeingDefined = this.m_bBeingDefined;
        return other;
    }
}

