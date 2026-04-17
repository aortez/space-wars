/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 * 
 * Could not load the following classes:
 *  javax.media.opengl.GL
 */
package UWBGL_JavaOpenGL;

import UWBGL_JavaOpenGL.UWBGL_Color;
import UWBGL_JavaOpenGL.UWBGL_Common;
import UWBGL_JavaOpenGL.UWBGL_DrawHelper;
import UWBGL_JavaOpenGL.UWBGL_EFillMode;
import UWBGL_JavaOpenGL.UWBGL_EShadeMode;
import UWBGL_JavaOpenGL.math3d.Vec3;
import javax.media.opengl.GL;

public class UWBGL_Xform {
    Vec3 m_translation;
    Vec3 m_scale;
    float m_rotation_radians;
    Vec3 m_pivot = new Vec3(0.0f, 0.0f, 0.0f);

    public UWBGL_Xform() {
        this.toIdentity();
    }

    public void toIdentity() {
        this.m_translation = new Vec3(0.0f, 0.0f, 0.0f);
        this.m_scale = new Vec3(1.0f, 1.0f, 1.0f);
        this.m_rotation_radians = 0.0f;
    }

    public void setUpModelStack(GL gl, UWBGL_DrawHelper draw_helper) {
        draw_helper.accumulateModelTransform(gl, this.m_translation, this.m_scale, this.m_rotation_radians, this.m_pivot);
    }

    public void drawPivot(GL gl, UWBGL_DrawHelper h, float size) {
        h.resetAttributes(gl);
        h.setColor1(UWBGL_Color.BLACK);
        h.setColor2(UWBGL_Color.WHITE);
        h.setFillMode(UWBGL_EFillMode.Solid);
        h.setShadeMode(UWBGL_EShadeMode.Gouraud);
        h.drawCircle(gl, this.m_pivot, size);
    }

    public Vec3 getTranslation() {
        return this.m_translation;
    }

    public void setTranslation(Vec3 translation) {
        this.m_translation = translation;
    }

    public void translateRotation(float f) {
        this.m_rotation_radians += f;
    }

    public void translateTranslation(Vec3 delta) {
        this.m_translation.plusEquals(delta);
    }

    public Vec3 getScale() {
        return this.m_scale;
    }

    public void setScale(Vec3 scale) {
        this.m_scale = scale;
    }

    public float getRotationInRadians() {
        return this.m_rotation_radians;
    }

    public float getRotationInDegrees() {
        return UWBGL_Common.toDegrees(this.m_rotation_radians);
    }

    public void setRotationRadians(float angle_in_radians) {
        this.m_rotation_radians = angle_in_radians;
    }

    public void setRotationDegrees(float angle_in_degrees) {
        this.m_rotation_radians = UWBGL_Common.toRadians(angle_in_degrees);
    }

    public Vec3 getPivot() {
        return this.m_pivot;
    }

    public void setPivot(Vec3 pivot) {
        this.m_pivot = pivot;
    }

    public String toString() {
        return "T:" + this.m_translation + " S:" + this.m_scale + " P:" + this.m_pivot + " R:" + this.m_rotation_radians + " D:" + this.getRotationInDegrees();
    }
}

