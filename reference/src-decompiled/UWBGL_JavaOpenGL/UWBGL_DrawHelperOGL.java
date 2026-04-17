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
import UWBGL_JavaOpenGL.UWBGL_Common;
import UWBGL_JavaOpenGL.UWBGL_DrawHelper;
import UWBGL_JavaOpenGL.UWBGL_EFillMode;
import UWBGL_JavaOpenGL.UWBGL_EShadeMode;
import UWBGL_JavaOpenGL.math3d.Vec3;
import java.nio.FloatBuffer;
import javax.media.opengl.GL;

public class UWBGL_DrawHelperOGL
extends UWBGL_DrawHelper {
    private static final int CIRCLE_HIGH_DETAIL = 40;
    private static final int CIRCLE_MEDIUM_DETAIL = 20;
    private static final int CIRCLE_LOW_DETAIL = 10;
    private static final Vec3[] circleVecsHighDetail = new Vec3[40];
    private static final Vec3[] circleVecsMediumDetail = new Vec3[20];
    private static final Vec3[] circleVecsLowDetail = new Vec3[10];
    private static final float[] cosHighDetail = new float[40];
    private static final float[] cosMediumDetail = new float[20];
    private static final float[] cosLowDetail = new float[10];
    private static final float[] sinHighDetail = new float[40];
    private static final float[] sinMediumDetail = new float[20];
    private static final float[] sinLowDetail = new float[10];
    private int kNumPts = 0;
    private Vec3[] v = null;
    private float[] cos = null;
    private float[] sin = null;

    public UWBGL_DrawHelperOGL() {
        float delta;
        int i;
        for (i = 0; i < circleVecsHighDetail.length; ++i) {
            delta = 0.16110732f;
            UWBGL_DrawHelperOGL.circleVecsHighDetail[i] = new Vec3();
            UWBGL_DrawHelperOGL.cosHighDetail[i] = (float)Math.cos((float)i * delta);
            UWBGL_DrawHelperOGL.sinHighDetail[i] = (float)Math.sin((float)i * delta);
        }
        for (i = 0; i < circleVecsMediumDetail.length; ++i) {
            delta = 0.33069396f;
            UWBGL_DrawHelperOGL.circleVecsMediumDetail[i] = new Vec3();
            UWBGL_DrawHelperOGL.cosMediumDetail[i] = (float)Math.cos((float)i * delta);
            UWBGL_DrawHelperOGL.sinMediumDetail[i] = (float)Math.sin((float)i * delta);
        }
        for (i = 0; i < circleVecsLowDetail.length; ++i) {
            delta = 0.69813174f;
            UWBGL_DrawHelperOGL.circleVecsLowDetail[i] = new Vec3();
            UWBGL_DrawHelperOGL.cosLowDetail[i] = (float)Math.cos((float)i * delta);
            UWBGL_DrawHelperOGL.sinLowDetail[i] = (float)Math.sin((float)i * delta);
        }
    }

    public void setShadeMode(GL gl, UWBGL_EShadeMode mode) {
        super.setShadeMode(mode);
        if (UWBGL_EShadeMode.Flat == mode) {
            gl.glShadeModel(7424);
        } else if (UWBGL_EShadeMode.Gouraud == mode) {
            gl.glShadeModel(7425);
        }
    }

    @Override
    public void setFillMode(UWBGL_EFillMode mode) {
        super.setFillMode(mode);
    }

    private void setFillMode(GL gl) {
        switch (this.m_FillMode) {
            case Solid: {
                gl.glPolygonMode(1032, 6914);
                break;
            }
            case Wireframe: 
            case Outline: {
                gl.glPolygonMode(1032, 6913);
                break;
            }
            default: {
                gl.glPolygonMode(1032, 6912);
            }
        }
    }

    @Override
    public boolean drawPoint(GL gl, Vec3 position) {
        gl.glPointSize(this.m_PointSize);
        gl.glBegin(0);
        this.m_Color1.applyTo(gl);
        gl.glVertex3f(position.x, position.y, position.z);
        gl.glEnd();
        gl.glPointSize(1.0f);
        return true;
    }

    @Override
    public boolean drawLine(GL gl, Vec3 start, Vec3 end) {
        gl.glBegin(1);
        this.m_Color1.applyTo(gl);
        gl.glVertex3f(start.x, start.y, start.z);
        if (UWBGL_EShadeMode.Gouraud == this.m_ShadeMode) {
            this.m_Color2.applyTo(gl);
        }
        gl.glVertex3f(end.x, end.y, end.z);
        gl.glEnd();
        return true;
    }

    @Override
    public boolean drawCircle(GL gl, Vec3 center, float radius) {
        if (radius <= 0.0f) {
            return false;
        }
        switch (this.m_lod) {
            case Low: {
                this.kNumPts = 10;
                this.v = circleVecsLowDetail;
                this.cos = cosLowDetail;
                this.sin = sinLowDetail;
                break;
            }
            case Medium: {
                this.kNumPts = 20;
                this.v = circleVecsMediumDetail;
                this.cos = cosMediumDetail;
                this.sin = sinMediumDetail;
                break;
            }
            default: {
                this.kNumPts = 40;
                this.v = circleVecsHighDetail;
                this.cos = cosHighDetail;
                this.sin = sinHighDetail;
            }
        }
        for (int i = 0; i < this.kNumPts; ++i) {
            this.v[i].x = center.x + radius * this.cos[i];
            this.v[i].y = center.y + radius * this.sin[i];
            this.v[i].z = center.z;
        }
        return this.drawPolygon(gl, center, this.v, center.x - radius, center.y - radius, radius * 2.0f, radius * 2.0f);
    }

    @Override
    public boolean drawTriangle(GL gl, Vec3[] points) {
        Vec3 center = points[0].midpoint(points[1], points[2]);
        return this.drawPolygon(gl, center, points);
    }

    @Override
    public boolean drawPolygon(GL gl, Vec3 center, Vec3[] points) {
        float minX = center.x;
        float maxX = center.x;
        float minY = center.y;
        float maxY = center.y;
        for (int i = 0; this.m_TextureEnabled && i < points.length; ++i) {
            minX = Math.min(points[i].x, minX);
            maxX = Math.max(points[i].x, maxX);
            minY = Math.min(points[i].y, minY);
            maxY = Math.max(points[i].y, maxY);
        }
        float xRange = maxX - minX;
        float yRange = maxY - minY;
        return this.drawPolygon(gl, center, points, minX, minY, xRange, yRange);
    }

    public boolean drawPolygon(GL gl, Vec3 center, Vec3[] points, float minX, float minY, float xRange, float yRange) {
        this.setFillMode(gl);
        if (UWBGL_EFillMode.Outline == this.m_FillMode) {
            gl.glBegin(2);
        } else {
            gl.glBegin(6);
        }
        if (UWBGL_EFillMode.Outline != this.m_FillMode) {
            if (UWBGL_EShadeMode.Gouraud == this.m_ShadeMode) {
                this.m_Color2.applyTo(gl);
            } else {
                this.m_Color1.applyTo(gl);
            }
            if (this.m_TextureEnabled) {
                gl.glTexCoord3f((center.x - minX) / xRange, (center.y - minY) / yRange, center.z);
            }
            gl.glVertex3f(center.x, center.y, center.z);
        }
        this.m_Color1.applyTo(gl);
        for (int i = 0; i < points.length; ++i) {
            if (this.m_TextureEnabled) {
                gl.glTexCoord3f((points[i].x - minX) / xRange, (points[i].y - minY) / yRange, center.z);
            }
            gl.glVertex3f(points[i].x, points[i].y, points[i].z);
        }
        if (this.m_TextureEnabled) {
            gl.glTexCoord3f((points[0].x - minX) / xRange, (points[0].y - minY) / yRange, center.z);
        }
        gl.glVertex3f(points[0].x, points[0].y, points[0].z);
        gl.glEnd();
        return true;
    }

    @Override
    public boolean drawRectangle(GL gl, Vec3 center, float width, float height) {
        Vec3[] points = new Vec3[]{new Vec3(center.x - width / 2.0f, center.y + height / 2.0f, center.z), new Vec3(center.x - width / 2.0f, center.y - height / 2.0f, center.z), new Vec3(center.x + width / 2.0f, center.y - height / 2.0f, center.z), new Vec3(center.x + width / 2.0f, center.y + height / 2.0f, center.z)};
        return this.drawPolygon(gl, center, points);
    }

    @Override
    public boolean drawRectangle(GL gl, Vec3 corner1, Vec3 corner2) {
        Vec3[] points = new Vec3[]{corner1, new Vec3(corner1.x, corner2.y, corner1.z), corner2, new Vec3(corner2.x, corner1.y, corner2.z)};
        Vec3 center = corner1.midpoint(corner2);
        return this.drawPolygon(gl, center, points);
    }

    @Override
    public boolean setBlending(GL gl, boolean on) {
        if (on) {
            gl.glEnable(3042);
            gl.glBlendFunc(770, 771);
        } else {
            gl.glDisable(3042);
        }
        this.m_BlendingEnabled = on;
        return true;
    }

    @Override
    public boolean setTexturing(GL gl, boolean on) {
        if (on) {
            this.m_TextureManager.activateTexture(gl, this.m_TexFileName);
        } else {
            this.m_TextureManager.deactivateTexture(gl);
        }
        this.m_TextureEnabled = on;
        return true;
    }

    @Override
    public boolean accumulateModelTransform(GL gl, Vec3 translation, Vec3 scale, float rotation_radians, Vec3 pivot) {
        gl.glTranslatef(translation.x, translation.y, translation.z);
        gl.glTranslatef(pivot.x, pivot.y, pivot.z);
        float degrees = UWBGL_Common.toDegrees(rotation_radians);
        gl.glRotatef(degrees, 0.0f, 0.0f, 1.0f);
        gl.glScalef(scale.x, scale.y, scale.z);
        gl.glTranslatef(-pivot.x, -pivot.y, -pivot.z);
        return true;
    }

    @Override
    public boolean pushModelTransform(GL gl) {
        gl.glMatrixMode(5888);
        gl.glPushMatrix();
        return true;
    }

    @Override
    public boolean popModelTransform(GL gl) {
        gl.glMatrixMode(5888);
        gl.glPopMatrix();
        return true;
    }

    @Override
    public boolean initializeModelTransform(GL gl) {
        gl.glMatrixMode(5888);
        gl.glLoadIdentity();
        return true;
    }

    @Override
    public Vec3 transformPoint(GL gl, Vec3 point) {
        FloatBuffer matrixBuffer = FloatBuffer.allocate(16);
        gl.glGetFloatv(2982, matrixBuffer);
        float[] matrix = matrixBuffer.array();
        float x = point.x * matrix[0] + point.y * matrix[4] + point.z * matrix[8] + matrix[12];
        float y = point.x * matrix[1] + point.y * matrix[5] + point.z * matrix[9] + matrix[13];
        float z = point.x * matrix[2] + point.y * matrix[6] + point.z * matrix[10] + matrix[14];
        return new Vec3(x, y, z);
    }

    public Vec3 transformPointEquals(GL gl, Vec3 point) {
        FloatBuffer matrixBuffer = FloatBuffer.allocate(16);
        gl.glGetFloatv(2982, matrixBuffer);
        float[] matrix = matrixBuffer.array();
        float x = point.x * matrix[0] + point.y * matrix[4] + point.z * matrix[8] + matrix[12];
        float y = point.x * matrix[1] + point.y * matrix[5] + point.z * matrix[9] + matrix[13];
        float z = point.x * matrix[2] + point.y * matrix[6] + point.z * matrix[10] + matrix[14];
        point.x = x;
        point.y = y;
        point.z = z;
        return point;
    }

    @Override
    public boolean transformBounds(GL gl, UWBGL_BoundingVolume bounds) {
        if (bounds.getType() == UWBGL_EVolumeType.List) {
            UWBGL_BoundingList list = (UWBGL_BoundingList)bounds;
            for (int i = 0; i < list.size(); ++i) {
                this.transformBounds(gl, list.get(i));
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
            this.transformPointEquals(gl, pt1);
            this.transformPointEquals(gl, pt2);
            this.transformPointEquals(gl, pt3);
            this.transformPointEquals(gl, pt4);
            this.transformPointEquals(gl, pt5);
            this.transformPointEquals(gl, pt6);
            this.transformPointEquals(gl, pt7);
            this.transformPointEquals(gl, pt8);
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
            Vec3 sideX = new Vec3(center.x + sphere.getRadius(), center.y, center.z);
            Vec3 sideY = new Vec3(center.x, center.y + sphere.getRadius(), center.z);
            Vec3 sideZ = new Vec3(center.x, center.y, center.z + sphere.getRadius());
            this.transformPointEquals(gl, center);
            this.transformPointEquals(gl, sideX);
            this.transformPointEquals(gl, sideY);
            this.transformPointEquals(gl, sideZ);
            float newRadius = Math.max(center.distanceTo(sideX), center.distanceTo(sideY));
            newRadius = Math.max(newRadius, center.distanceTo(sideZ));
            sphere.setRadius(newRadius);
            return true;
        }
        return false;
    }
}

