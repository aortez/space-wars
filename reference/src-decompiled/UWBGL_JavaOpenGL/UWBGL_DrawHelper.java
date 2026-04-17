/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 * 
 * Could not load the following classes:
 *  javax.media.opengl.GL
 */
package UWBGL_JavaOpenGL;

import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingBox;
import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingList;
import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingSphere;
import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingVolume;
import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_EVolumeType;
import UWBGL_JavaOpenGL.UWBGL_Color;
import UWBGL_JavaOpenGL.UWBGL_EFillMode;
import UWBGL_JavaOpenGL.UWBGL_ELevelOfDetail;
import UWBGL_JavaOpenGL.UWBGL_EShadeMode;
import UWBGL_JavaOpenGL.UWBGL_TextureManager;
import UWBGL_JavaOpenGL.UWBGL_Xform;
import UWBGL_JavaOpenGL.math3d.Vec3;
import java.util.LinkedList;
import javax.media.opengl.GL;

public abstract class UWBGL_DrawHelper {
    protected UWBGL_ELevelOfDetail m_lod;
    protected UWBGL_EShadeMode m_ShadeMode;
    protected UWBGL_EFillMode m_FillMode;
    protected float m_PointSize = 1.0f;
    protected UWBGL_Color m_Color1;
    protected UWBGL_Color m_Color2;
    protected String m_TexFileName = "";
    protected UWBGL_TextureManager m_TextureManager;
    protected boolean m_TextureEnabled = false;
    protected boolean m_BlendingEnabled = false;
    private LinkedList<UWBGL_Xform> m_XformStack = new LinkedList();
    private Vec3 sideX = new Vec3();
    private Vec3 sideY = new Vec3();
    private Vec3 sideZ = new Vec3();

    public UWBGL_DrawHelper() {
        this.m_TextureManager = new UWBGL_TextureManager();
        this.m_lod = UWBGL_ELevelOfDetail.High;
        this.m_ShadeMode = UWBGL_EShadeMode.Gouraud;
        this.m_FillMode = UWBGL_EFillMode.Solid;
        this.m_Color1 = UWBGL_Color.BLACK;
        this.m_Color2 = UWBGL_Color.BLACK;
    }

    public void resetAttributes(GL gl) {
        this.m_lod = UWBGL_ELevelOfDetail.High;
        this.m_ShadeMode = UWBGL_EShadeMode.Gouraud;
        this.m_FillMode = UWBGL_EFillMode.Solid;
        this.m_PointSize = 1.0f;
        this.m_Color1 = UWBGL_Color.BLACK;
        this.m_Color2 = UWBGL_Color.BLACK;
        this.m_TexFileName = "";
        if (this.m_TextureEnabled) {
            this.setTexturing(gl, false);
        }
        if (this.m_BlendingEnabled) {
            this.setBlending(gl, false);
        }
    }

    public void setLOD(UWBGL_ELevelOfDetail lod) {
        this.m_lod = lod;
    }

    public UWBGL_ELevelOfDetail getLOD() {
        return this.m_lod;
    }

    public void setShadeMode(UWBGL_EShadeMode mode) {
        this.m_ShadeMode = mode;
    }

    public UWBGL_EShadeMode getShadeMode() {
        return this.m_ShadeMode;
    }

    public void setFillMode(UWBGL_EFillMode mode) {
        this.m_FillMode = mode;
    }

    public UWBGL_EFillMode getFillMode() {
        return this.m_FillMode;
    }

    public UWBGL_Color setColor1(UWBGL_Color color) {
        UWBGL_Color old = this.m_Color1;
        this.m_Color1 = color;
        return old;
    }

    public UWBGL_Color setColor2(UWBGL_Color color) {
        UWBGL_Color old = this.m_Color2;
        this.m_Color2 = color;
        return old;
    }

    public float setPointSize(float pt_size) {
        float old = this.m_PointSize;
        this.m_PointSize = pt_size;
        return old;
    }

    public void setTextureInfo(String texFile) {
        this.m_TexFileName = texFile;
    }

    public abstract boolean drawPoint(GL var1, Vec3 var2);

    public abstract boolean drawLine(GL var1, Vec3 var2, Vec3 var3);

    public abstract boolean drawCircle(GL var1, Vec3 var2, float var3);

    public abstract boolean drawRectangle(GL var1, Vec3 var2, float var3, float var4);

    public abstract boolean drawTriangle(GL var1, Vec3[] var2);

    public abstract boolean drawPolygon(GL var1, Vec3 var2, Vec3[] var3);

    public abstract boolean drawRectangle(GL var1, Vec3 var2, Vec3 var3);

    public abstract boolean setBlending(GL var1, boolean var2);

    public abstract boolean setTexturing(GL var1, boolean var2);

    public abstract boolean accumulateModelTransform(GL var1, Vec3 var2, Vec3 var3, float var4, Vec3 var5);

    public abstract boolean pushModelTransform(GL var1);

    public abstract boolean popModelTransform(GL var1);

    public abstract boolean initializeModelTransform(GL var1);

    public abstract Vec3 transformPoint(GL var1, Vec3 var2);

    public abstract boolean transformBounds(GL var1, UWBGL_BoundingVolume var2);

    public void loadIdentity() {
        this.m_XformStack.clear();
    }

    public void pushModelTransform() {
    }

    public void accumulateModelTransform(UWBGL_Xform xform) {
        this.m_XformStack.addFirst(xform);
    }

    public void popModelTransform() {
        this.m_XformStack.removeFirst();
    }

    public Vec3 transformPointEquals(Vec3 point) {
        for (int i = 0; i < this.m_XformStack.size(); ++i) {
            UWBGL_Xform xform = this.m_XformStack.get(i);
            point.minusEquals(xform.getPivot());
            point.rotateREquals(xform.getRotationInRadians());
            point.timesEquals(xform.getScale());
            point.plusEquals(xform.getPivot());
            point.plusEquals(xform.getTranslation());
        }
        return point;
    }

    public synchronized boolean transformBounds(UWBGL_BoundingVolume bounds) {
        if (bounds.getType() == UWBGL_EVolumeType.List) {
            UWBGL_BoundingList list = (UWBGL_BoundingList)bounds;
            for (int i = 0; i < list.size(); ++i) {
                this.transformBounds(list.get(i));
            }
            return true;
        }
        if (bounds.getType() == UWBGL_EVolumeType.Box) {
            UWBGL_BoundingBox box = (UWBGL_BoundingBox)bounds;
            Vec3 minPt = box.getMin();
            Vec3 maxPt = box.getMax();
            Vec3 pt1 = new Vec3(minPt.x, minPt.y, minPt.z);
            Vec3 pt2 = new Vec3(maxPt.x, minPt.y, minPt.z);
            Vec3 pt3 = new Vec3(maxPt.x, maxPt.y, minPt.z);
            Vec3 pt4 = new Vec3(minPt.x, maxPt.y, minPt.z);
            Vec3 pt5 = new Vec3(minPt.x, minPt.y, maxPt.z);
            Vec3 pt6 = new Vec3(maxPt.x, minPt.y, maxPt.z);
            Vec3 pt7 = new Vec3(maxPt.x, maxPt.y, maxPt.z);
            Vec3 pt8 = new Vec3(minPt.x, maxPt.y, maxPt.z);
            this.transformPointEquals(pt1);
            this.transformPointEquals(pt2);
            this.transformPointEquals(pt3);
            this.transformPointEquals(pt4);
            this.transformPointEquals(pt5);
            this.transformPointEquals(pt6);
            this.transformPointEquals(pt7);
            this.transformPointEquals(pt8);
            box.makeInvalid();
            box.add(new UWBGL_BoundingBox(pt1, pt2));
            box.add(new UWBGL_BoundingBox(pt3, pt4));
            box.add(new UWBGL_BoundingBox(pt5, pt6));
            box.add(new UWBGL_BoundingBox(pt7, pt8));
            return true;
        }
        if (bounds.getType() == UWBGL_EVolumeType.Sphere) {
            UWBGL_BoundingSphere sphere = (UWBGL_BoundingSphere)bounds;
            Vec3 center = sphere.getCenter();
            this.sideX.set(center.x + sphere.getRadius(), center.y, center.z);
            this.sideY.set(center.x, center.y + sphere.getRadius(), center.z);
            this.sideZ.set(center.x, center.y, center.z + sphere.getRadius());
            this.transformPointEquals(center);
            this.transformPointEquals(this.sideX);
            this.transformPointEquals(this.sideY);
            this.transformPointEquals(this.sideZ);
            float newRadius = Math.max(center.distanceTo(this.sideX), center.distanceTo(this.sideY));
            newRadius = Math.max(newRadius, center.distanceTo(this.sideZ));
            sphere.setRadius(newRadius);
            return true;
        }
        return false;
    }
}

