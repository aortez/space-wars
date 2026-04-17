/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 * 
 * Could not load the following classes:
 *  javax.media.opengl.GL
 */
package UWBGL_JavaOpenGL.BoundingVolumes;

import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingVolume;
import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_EVolumeType;
import UWBGL_JavaOpenGL.UWBGL_DrawHelper;
import UWBGL_JavaOpenGL.math3d.Vec3;
import java.util.ArrayList;
import java.util.Iterator;
import javax.media.opengl.GL;

public class UWBGL_BoundingList
implements UWBGL_BoundingVolume {
    private ArrayList<UWBGL_BoundingVolume> m_NestedBounds = new ArrayList();

    @Override
    public UWBGL_EVolumeType getType() {
        return UWBGL_EVolumeType.List;
    }

    @Override
    public Vec3 getCenter() {
        Vec3 sum = new Vec3(0.0f, 0.0f, 0.0f);
        Iterator<UWBGL_BoundingVolume> i = this.m_NestedBounds.iterator();
        UWBGL_BoundingVolume current = null;
        while (i.hasNext()) {
            current = i.next();
            sum.plusEquals(current.getCenter());
        }
        sum.divideEquals(this.m_NestedBounds.size());
        return sum;
    }

    @Override
    public void add(UWBGL_BoundingVolume pBV) {
        switch (pBV.getType()) {
            case List: {
                UWBGL_BoundingList list = (UWBGL_BoundingList)pBV;
                for (int i = 0; i < list.size(); ++i) {
                    this.add(list.get(i));
                }
                break;
            }
            default: {
                this.m_NestedBounds.add(pBV);
            }
        }
    }

    @Override
    public boolean intersects(UWBGL_BoundingVolume other) {
        Iterator<UWBGL_BoundingVolume> i = this.m_NestedBounds.iterator();
        UWBGL_BoundingVolume current = null;
        while (i.hasNext()) {
            current = i.next();
            if (!current.intersects(other)) continue;
            return true;
        }
        return false;
    }

    @Override
    public Vec3 isContainedBy(Vec3 min, Vec3 max) {
        Iterator<UWBGL_BoundingVolume> i = this.m_NestedBounds.iterator();
        UWBGL_BoundingVolume current = null;
        Vec3 fudge = new Vec3(0.0f, 0.0f, 0.0f);
        while (i.hasNext()) {
            current = i.next();
            fudge.plusEquals(current.isContainedBy(min, max));
        }
        return fudge;
    }

    @Override
    public Vec3 isContainedBy(Vec3 center, float radius) {
        Iterator<UWBGL_BoundingVolume> i = this.m_NestedBounds.iterator();
        UWBGL_BoundingVolume current = null;
        Vec3 fudge = new Vec3(0.0f, 0.0f, 0.0f);
        while (i.hasNext()) {
            current = i.next();
            fudge.plusEquals(current.isContainedBy(center, radius));
        }
        return fudge;
    }

    @Override
    public boolean containsPoint(Vec3 test_point) {
        Iterator<UWBGL_BoundingVolume> i = this.m_NestedBounds.iterator();
        UWBGL_BoundingVolume current = null;
        while (i.hasNext()) {
            current = i.next();
            if (!current.containsPoint(test_point)) continue;
            return true;
        }
        return false;
    }

    @Override
    public void draw(GL gl, UWBGL_DrawHelper pDrawHelper) {
        Iterator<UWBGL_BoundingVolume> i = this.m_NestedBounds.iterator();
        UWBGL_BoundingVolume current = null;
        while (i.hasNext()) {
            current = i.next();
            current.draw(gl, pDrawHelper);
        }
    }

    public int size() {
        return this.m_NestedBounds.size();
    }

    public UWBGL_BoundingVolume get(int index) {
        return this.m_NestedBounds.get(index);
    }
}

