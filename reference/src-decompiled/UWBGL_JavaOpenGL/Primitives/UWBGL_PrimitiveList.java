/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 * 
 * Could not load the following classes:
 *  javax.media.opengl.GL
 */
package UWBGL_JavaOpenGL.Primitives;

import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingVolume;
import UWBGL_JavaOpenGL.Primitives.UWBGL_Primitive;
import UWBGL_JavaOpenGL.UWBGL_Color;
import UWBGL_JavaOpenGL.UWBGL_DrawHelper;
import UWBGL_JavaOpenGL.UWBGL_EFillMode;
import UWBGL_JavaOpenGL.UWBGL_ELevelOfDetail;
import UWBGL_JavaOpenGL.UWBGL_EShadeMode;
import UWBGL_JavaOpenGL.math3d.Vec3;
import java.util.ArrayList;
import javax.media.opengl.GL;

public class UWBGL_PrimitiveList
extends UWBGL_Primitive {
    private ArrayList<UWBGL_Primitive> m_list = new ArrayList();

    public void remove(int index) {
        this.m_list.remove(index);
    }

    @Override
    public void setFlatColor(UWBGL_Color color) {
        for (UWBGL_Primitive p : this.m_list) {
            p.setFlatColor(color);
        }
    }

    @Override
    public void setupDrawAttributes(GL gl, UWBGL_DrawHelper draw_helper) {
    }

    @Override
    public void drawPrimitive(GL gl, UWBGL_ELevelOfDetail lod, UWBGL_DrawHelper draw_helper) {
        for (UWBGL_Primitive p : this.m_list) {
            p.draw(gl, lod, draw_helper);
        }
    }

    @Override
    public boolean hasPrimitive(UWBGL_Primitive other) {
        if (super.hasPrimitive(other)) {
            return true;
        }
        for (UWBGL_Primitive p : this.m_list) {
            if (!p.hasPrimitive(other)) continue;
            return true;
        }
        return false;
    }

    @Override
    public UWBGL_BoundingVolume getBoundingVolume(UWBGL_ELevelOfDetail lod) {
        int count = this.m_list.size();
        if (0 == count) {
            return null;
        }
        UWBGL_Primitive pPrimitive = this.m_list.get(0);
        UWBGL_BoundingVolume pTotalBv = pPrimitive.getBoundingVolume(lod);
        for (int i = 1; i < count; ++i) {
            pPrimitive = this.m_list.get(i);
            UWBGL_BoundingVolume pBV = pPrimitive.getBoundingVolume(lod);
            pTotalBv.add(pBV);
        }
        return pTotalBv;
    }

    public int size() {
        return this.m_list.size();
    }

    public UWBGL_Primitive get(int index) {
        if (index < 0 || index >= this.m_list.size()) {
            return null;
        }
        return this.m_list.get(index);
    }

    public void add(UWBGL_Primitive pPrimitive) {
        this.m_list.add(pPrimitive);
    }

    public void deletePrimitiveAt(int index) {
        this.m_list.remove(index);
    }

    @Override
    public void setShadeMode(UWBGL_EShadeMode mode) {
        for (UWBGL_Primitive p : this.m_list) {
            p.setShadeMode(mode);
        }
    }

    @Override
    public void setShadingColor(UWBGL_Color color) {
        for (UWBGL_Primitive p : this.m_list) {
            p.setShadingColor(color);
        }
    }

    @Override
    public void setFillMode(UWBGL_EFillMode mode) {
        for (UWBGL_Primitive p : this.m_list) {
            p.setFillMode(mode);
        }
    }

    @Override
    public void setBlending(boolean on) {
        for (UWBGL_Primitive p : this.m_list) {
            p.setBlending(on);
        }
    }

    public void drawChildBoundingVolumes(GL gl, UWBGL_ELevelOfDetail lod, UWBGL_DrawHelper pDrawHelper, UWBGL_Color color) {
        for (UWBGL_Primitive p : this.m_list) {
            p.drawBoundingVolume(gl, lod, pDrawHelper, color);
        }
    }

    @Override
    public void moveTo(Vec3 loc) {
        throw new UnsupportedOperationException("Not supported yet.");
    }

    @Override
    public UWBGL_Primitive clone() {
        throw new UnsupportedOperationException("Not supported yet.");
    }

    @Override
    public void definitionAction(int definitionAction, float x, float y) {
        throw new UnsupportedOperationException("Not supported yet.");
    }

    @Override
    public Vec3 getLocation() {
        Vec3 center = new Vec3();
        if (this.m_list.size() <= 0) {
            return center;
        }
        for (UWBGL_Primitive p : this.m_list) {
            center.plusEquals(p.getLocation().times(p.getSize()));
        }
        center.divideEquals(this.getSize());
        return center;
    }

    @Override
    public float getSize() {
        float size = 0.0f;
        if (this.m_list.size() <= 0) {
            return size;
        }
        for (UWBGL_Primitive p : this.m_list) {
            size += p.getSize();
        }
        return size;
    }

    @Override
    public void setSize(float size) {
        throw new UnsupportedOperationException("Not supported yet.");
    }

    @Override
    public float getArea() {
        float area = 0.0f;
        for (UWBGL_Primitive p : this.m_list) {
            area += p.getArea();
        }
        return area;
    }
}

