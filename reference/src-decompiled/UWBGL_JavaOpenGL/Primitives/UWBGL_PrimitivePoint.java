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

public class UWBGL_PrimitivePoint
extends UWBGL_Primitive {
    private Vec3 m_center;

    public UWBGL_PrimitivePoint() {
        this(new Vec3(0.0f, 0.0f, 0.0f), 2.0f);
        this.m_bBeingDefined = true;
    }

    public UWBGL_PrimitivePoint(Vec3 center, float pointSize) {
        this.m_center = center;
        this.m_PointSize = pointSize;
        this.m_bBeingDefined = false;
    }

    public void setLocation(Vec3 center) {
        this.m_center = center;
    }

    @Override
    public Vec3 getLocation() {
        return this.m_center;
    }

    @Override
    public void setSize(float pointSize) {
        this.m_PointSize = pointSize;
    }

    @Override
    public void moveTo(Vec3 loc) {
        this.m_center = loc;
    }

    @Override
    public UWBGL_BoundingVolume getBoundingVolume(UWBGL_ELevelOfDetail lod) {
        return new UWBGL_BoundingSphere(this.m_center, this.m_PointSize * 0.99f / 2.0f);
    }

    @Override
    protected void drawPrimitive(GL gl, UWBGL_ELevelOfDetail lod, UWBGL_DrawHelper draw_helper) {
        UWBGL_ELevelOfDetail oldLod = draw_helper.getLOD();
        draw_helper.setLOD(lod);
        draw_helper.setPointSize(this.m_PointSize);
        draw_helper.drawPoint(gl, this.m_center);
        draw_helper.setLOD(oldLod);
    }

    @Override
    public UWBGL_Primitive clone() {
        UWBGL_PrimitivePoint other = new UWBGL_PrimitivePoint(this.m_center, this.m_PointSize);
        other.m_bBeingDefined = this.m_bBeingDefined;
        return other;
    }

    @Override
    public void definitionAction(int definitionAction, float x, float y) {
        if (this.m_bBeingDefined && definitionAction == 0) {
            this.setLocation(new Vec3(x, y, 0.0f));
            this.m_bBeingDefined = false;
        }
    }

    @Override
    public float getSize() {
        return this.m_PointSize;
    }

    @Override
    public float getArea() {
        return 1.0E-4f;
    }
}

