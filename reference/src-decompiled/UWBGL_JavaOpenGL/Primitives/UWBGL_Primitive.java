/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 * 
 * Could not load the following classes:
 *  javax.media.opengl.GL
 */
package UWBGL_JavaOpenGL.Primitives;

import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingVolume;
import UWBGL_JavaOpenGL.UWBGL_Color;
import UWBGL_JavaOpenGL.UWBGL_DrawHelper;
import UWBGL_JavaOpenGL.UWBGL_EFillMode;
import UWBGL_JavaOpenGL.UWBGL_ELevelOfDetail;
import UWBGL_JavaOpenGL.UWBGL_EShadeMode;
import UWBGL_JavaOpenGL.math3d.Vec3;
import javax.media.opengl.GL;

public abstract class UWBGL_Primitive {
    public static final float BOUNDING_VOULUME_SCALE = 0.99f;
    public static final float RAND_SPEED_MIN = 10.0f;
    public static final float RAND_SPEED_MAX = 200.0f;
    public static final int DEFINE_ACTION_CREATE = 0;
    public static final int DEFINE_ACTION_ADJUST = 1;
    public static final int DEFINE_ACTION_SET = 2;
    protected boolean m_bVisible = true;
    protected UWBGL_Color m_FlatColor;
    protected UWBGL_Color m_ShadingColor;
    protected UWBGL_EFillMode m_FillMode = UWBGL_EFillMode.Wireframe;
    protected UWBGL_EShadeMode m_ShadeMode = UWBGL_EShadeMode.Flat;
    protected float m_PointSize = 2.0f;
    protected boolean m_bDrawBoundingVolume = false;
    protected boolean m_bBeingDefined = true;
    protected boolean m_BlendingEnabled = false;
    protected String m_TexFileName = "";
    protected boolean m_TexturingEnabled = false;

    public UWBGL_Primitive() {
        this.m_FlatColor = UWBGL_Color.BLACK;
        this.m_ShadingColor = UWBGL_Color.WHITE;
    }

    public void draw(GL gl, UWBGL_ELevelOfDetail lod, UWBGL_DrawHelper draw_helper) {
        if (!this.isVisible()) {
            return;
        }
        this.setupDrawAttributes(gl, draw_helper);
        this.drawPrimitive(gl, lod, draw_helper);
        if (this.isDrawBoundingVolume()) {
            this.drawBoundingVolume(gl, lod, draw_helper, UWBGL_Color.RED);
        }
    }

    public abstract void definitionAction(int var1, float var2, float var3);

    public abstract void moveTo(Vec3 var1);

    public void moveBy(Vec3 offset) {
        this.moveTo(offset.plus(this.getLocation()));
    }

    public abstract Vec3 getLocation();

    public abstract float getArea();

    public abstract float getSize();

    public abstract void setSize(float var1);

    public void adjustSize(float delta) {
        this.setSize(this.getSize() + delta);
    }

    protected void setupDrawAttributes(GL gl, UWBGL_DrawHelper draw_helper) {
        draw_helper.setColor1(this.m_FlatColor);
        draw_helper.setColor2(this.m_ShadingColor);
        draw_helper.setShadeMode(this.m_ShadeMode);
        draw_helper.setFillMode(this.m_FillMode);
        draw_helper.setBlending(gl, this.m_BlendingEnabled);
        draw_helper.setTextureInfo(this.m_TexFileName);
        draw_helper.setTexturing(gl, this.m_TexturingEnabled);
    }

    public void setVisible(boolean on) {
        this.m_bVisible = on;
    }

    public boolean isVisible() {
        return this.m_bVisible;
    }

    public void setDrawBoundingVolume(boolean on) {
        this.m_bDrawBoundingVolume = on;
    }

    public boolean isDrawBoundingVolume() {
        return this.m_bDrawBoundingVolume;
    }

    public void setFlatColor(UWBGL_Color color) {
        this.m_FlatColor = color;
    }

    public UWBGL_Color getFlatColor() {
        return this.m_FlatColor;
    }

    public void setShadingColor(UWBGL_Color color) {
        this.m_ShadingColor = color;
    }

    public UWBGL_Color getShadingColor() {
        return this.m_ShadingColor;
    }

    public void setFillMode(UWBGL_EFillMode mode) {
        this.m_FillMode = mode;
    }

    public UWBGL_EFillMode getFillMode() {
        return this.m_FillMode;
    }

    public void setShadeMode(UWBGL_EShadeMode mode) {
        this.m_ShadeMode = mode;
    }

    public UWBGL_EShadeMode getShadeMode() {
        return this.m_ShadeMode;
    }

    public void setPointSize(float sz) {
        this.m_PointSize = sz;
    }

    public float getPointSize() {
        return this.m_PointSize;
    }

    public boolean hasPrimitive(UWBGL_Primitive other) {
        return this == other;
    }

    public abstract UWBGL_BoundingVolume getBoundingVolume(UWBGL_ELevelOfDetail var1);

    public boolean hasCollidedWith(UWBGL_Primitive pOther) {
        UWBGL_BoundingVolume pOtherBounds;
        UWBGL_BoundingVolume pBounds = this.getBoundingVolume(UWBGL_ELevelOfDetail.Low);
        if (!pBounds.intersects(pOtherBounds = pOther.getBoundingVolume(UWBGL_ELevelOfDetail.Low))) {
            return false;
        }
        pBounds = this.getBoundingVolume(UWBGL_ELevelOfDetail.Medium);
        if (!pBounds.intersects(pOtherBounds = pOther.getBoundingVolume(UWBGL_ELevelOfDetail.Medium))) {
            return false;
        }
        pBounds = this.getBoundingVolume(UWBGL_ELevelOfDetail.High);
        pOtherBounds = pOther.getBoundingVolume(UWBGL_ELevelOfDetail.High);
        return pBounds.intersects(pOtherBounds);
    }

    public void drawBoundingVolume(GL gl, UWBGL_ELevelOfDetail lod, UWBGL_DrawHelper pRenderer, UWBGL_Color color) {
        UWBGL_BoundingVolume pBounds = this.getBoundingVolume(lod);
        if (pBounds != null && pRenderer != null) {
            pRenderer.resetAttributes(gl);
            pRenderer.setColor1(color);
            pRenderer.setShadeMode(UWBGL_EShadeMode.Flat);
            pRenderer.setFillMode(UWBGL_EFillMode.Wireframe);
            pBounds.draw(gl, pRenderer);
        }
    }

    public void setBlending(boolean on) {
        this.m_BlendingEnabled = on;
    }

    public boolean getBlendingEnabled() {
        return this.m_BlendingEnabled;
    }

    public void setTextureFileName(String n) {
        this.m_TexFileName = n;
    }

    public String getTextureFileName() {
        return this.m_TexFileName;
    }

    public void setTexturing(boolean on) {
        this.m_TexturingEnabled = on;
    }

    public boolean getTexturingEnabled() {
        return this.m_TexturingEnabled;
    }

    public boolean isBeingDefined() {
        return this.m_bBeingDefined;
    }

    public abstract UWBGL_Primitive clone();

    protected abstract void drawPrimitive(GL var1, UWBGL_ELevelOfDetail var2, UWBGL_DrawHelper var3);
}

