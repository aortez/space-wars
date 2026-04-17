/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 * 
 * Could not load the following classes:
 *  javax.media.opengl.GL
 */
package UWBGL_JavaOpenGL.BoundingVolumes;

import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingBox;
import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingLine;
import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingVolume;
import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_EVolumeType;
import UWBGL_JavaOpenGL.UWBGL_Color;
import UWBGL_JavaOpenGL.UWBGL_Common;
import UWBGL_JavaOpenGL.UWBGL_DrawHelper;
import UWBGL_JavaOpenGL.UWBGL_EFillMode;
import UWBGL_JavaOpenGL.UWBGL_EShadeMode;
import UWBGL_JavaOpenGL.math3d.Vec3;
import javax.media.opengl.GL;

public class UWBGL_BoundingSphere
implements UWBGL_BoundingVolume {
    private Vec3 lowerBound = new Vec3();
    private Vec3 upperBound = new Vec3();
    private Vec3 m_center = new Vec3(0.0f, 0.0f, 0.0f);
    private float m_radius = 0.0f;

    public UWBGL_BoundingSphere() {
    }

    public UWBGL_BoundingSphere(Vec3 center, float radius) {
        this();
        this.moveCenterTo(center);
        this.setRadius(radius);
    }

    @Override
    public UWBGL_EVolumeType getType() {
        return UWBGL_EVolumeType.Sphere;
    }

    public void moveCenterTo(Vec3 new_center) {
        this.m_center = new_center;
    }

    @Override
    public boolean intersects(UWBGL_BoundingVolume other) {
        switch (other.getType()) {
            case Sphere: {
                UWBGL_BoundingSphere pOtherSphere = (UWBGL_BoundingSphere)other;
                return UWBGL_Common.intersectSphereSphere(this.m_center, this.m_radius, pOtherSphere.getCenter(), pOtherSphere.getRadius());
            }
            case List: {
                return other.intersects(this);
            }
            case Box: {
                UWBGL_BoundingBox box = (UWBGL_BoundingBox)other;
                return UWBGL_Common.intersectSphereBox(this.getCenter(), this.getRadius(), box.getMin(), box.getMax());
            }
            case Line: {
                UWBGL_BoundingLine line = (UWBGL_BoundingLine)other;
                return UWBGL_Common.intersectLineSphere(line.getHead(), line.getTail(), this.m_center, this.m_radius);
            }
        }
        System.out.println("UWBGL_BoundingSphere: unknown intersection type: " + (Object)((Object)this.getType()) + " to " + (Object)((Object)other.getType()));
        System.exit(-1);
        return false;
    }

    @Override
    public Vec3 getCenter() {
        return this.m_center;
    }

    public void setCenter(Vec3 c) {
        this.m_center = c;
    }

    @Override
    public boolean containsPoint(Vec3 location) {
        float distance = this.m_center.length();
        return !(distance >= this.m_radius);
    }

    @Override
    public void draw(GL gl, UWBGL_DrawHelper pDrawHelper) {
        pDrawHelper.resetAttributes(gl);
        pDrawHelper.setColor1(UWBGL_Color.RED);
        pDrawHelper.setShadeMode(UWBGL_EShadeMode.Flat);
        pDrawHelper.setFillMode(UWBGL_EFillMode.Wireframe);
        pDrawHelper.drawCircle(gl, this.m_center, this.m_radius);
    }

    @Override
    public void add(UWBGL_BoundingVolume pBV) {
        if (pBV != null) {
            UWBGL_EVolumeType vt = pBV.getType();
            switch (vt) {
                case Sphere: {
                    UWBGL_BoundingSphere pSphere = (UWBGL_BoundingSphere)pBV;
                    this.add(pSphere);
                }
            }
        }
    }

    public void add(UWBGL_BoundingSphere other) {
        float distance = this.m_center.distanceTo(other.m_center);
        if (distance + other.m_radius > this.m_radius) {
            this.m_radius = distance + other.m_radius;
        }
    }

    public void setRadius(float r) {
        this.m_radius = r;
    }

    public float getRadius() {
        return this.m_radius;
    }

    @Override
    public Vec3 isContainedBy(Vec3 min, Vec3 max) {
        Vec3 fudgeVector = new Vec3(0.0f, 0.0f, 0.0f);
        this.lowerBound.set(this.m_center.x - this.m_radius, this.m_center.y - this.m_radius, this.m_center.z - this.m_radius);
        this.upperBound.set(this.m_center.x + this.m_radius, this.m_center.y + this.m_radius, this.m_center.z + this.m_radius);
        if (this.lowerBound.x < min.x) {
            fudgeVector.x = min.x - this.lowerBound.x;
        } else if (this.upperBound.x > max.x) {
            fudgeVector.x = max.x - this.upperBound.x;
        }
        if (this.lowerBound.y < min.y) {
            fudgeVector.y = min.y - this.lowerBound.y;
        } else if (this.upperBound.y > max.y) {
            fudgeVector.y = max.y - this.upperBound.y;
        }
        return fudgeVector;
    }

    @Override
    public Vec3 isContainedBy(Vec3 center, float radius) {
        float distance = this.m_center.distanceTo(center);
        if (distance + this.m_radius > radius) {
            return center.minus(this.m_center).normalizedEquals().timesEquals(distance - radius + this.m_radius);
        }
        return new Vec3();
    }
}

