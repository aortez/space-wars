/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 * 
 * Could not load the following classes:
 *  javax.media.opengl.GL
 */
package UWBGL_JavaOpenGL;

import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingList;
import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingSphere;
import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingVolume;
import UWBGL_JavaOpenGL.Primitives.UWBGL_Primitive;
import UWBGL_JavaOpenGL.UWBGL_DrawHelper;
import UWBGL_JavaOpenGL.UWBGL_ELevelOfDetail;
import UWBGL_JavaOpenGL.UWBGL_Xform;
import UWBGL_JavaOpenGL.math3d.Vec3;
import java.util.ArrayList;
import javax.media.opengl.GL;

public class UWBGL_SceneNode {
    private UWBGL_Primitive m_Primitive;
    private ArrayList<UWBGL_SceneNode> m_ChildNodes;
    protected Vec3 m_Velocity;
    private UWBGL_Xform m_Xform;
    private boolean m_PivotIsVisible;
    private String m_Name;

    public UWBGL_SceneNode(String name) {
        this.m_Name = name;
        this.m_Velocity = new Vec3();
        this.m_PivotIsVisible = false;
        this.m_ChildNodes = new ArrayList();
        this.m_Primitive = null;
        this.m_Xform = new UWBGL_Xform();
    }

    public void draw(GL gl, UWBGL_ELevelOfDetail lod, UWBGL_DrawHelper draw_helper) {
        draw_helper.pushModelTransform(gl);
        this.m_Xform.setUpModelStack(gl, draw_helper);
        if (this.m_Primitive != null) {
            this.m_Primitive.draw(gl, lod, draw_helper);
        }
        int count = this.m_ChildNodes.size();
        for (int i = 0; i < count; ++i) {
            this.m_ChildNodes.get(i).draw(gl, lod, draw_helper);
        }
        if (this.m_PivotIsVisible) {
            this.m_Xform.drawPivot(gl, draw_helper, 0.2f);
        }
        draw_helper.popModelTransform(gl);
    }

    public void setPrimitive(UWBGL_Primitive primitive) {
        this.m_Primitive = primitive;
    }

    public UWBGL_Primitive getPrimitive() {
        return this.m_Primitive;
    }

    public void insertChildNode(UWBGL_SceneNode pChild) {
        this.m_ChildNodes.add(pChild);
    }

    public int numChildren() {
        return this.m_ChildNodes.size();
    }

    public UWBGL_SceneNode getChildNode(int index) {
        if (index < 0 || index >= this.numChildren()) {
            return null;
        }
        return this.m_ChildNodes.get(index);
    }

    public void moveNodeByVelocity(float elapsed_seconds) {
        this.m_Xform.translateTranslation(this.m_Velocity.times(elapsed_seconds));
    }

    public void setVelocity(Vec3 v) {
        this.m_Velocity = v;
    }

    public void translateVelocity(Vec3 delta_v) {
        this.m_Velocity.plusEquals(delta_v);
    }

    Vec3 getVelocity() {
        return this.m_Velocity;
    }

    public String getName() {
        return this.m_Name;
    }

    public UWBGL_Xform getXform() {
        return this.m_Xform;
    }

    public void setXform(UWBGL_Xform xform) {
        this.m_Xform = xform;
    }

    public void setPivotVisible(boolean on) {
        this.m_PivotIsVisible = on;
    }

    public boolean isPivotVisible() {
        return this.m_PivotIsVisible;
    }

    public UWBGL_BoundingVolume getBounds(GL gl, UWBGL_ELevelOfDetail lod, UWBGL_DrawHelper helper, boolean bDraw) {
        UWBGL_BoundingVolume bounds = null;
        if (lod == UWBGL_ELevelOfDetail.Low) {
            Vec3 boundsCenter = null;
            boundsCenter = this.m_Primitive != null ? this.m_Primitive.getLocation().clone() : new Vec3();
            bounds = new UWBGL_BoundingSphere(boundsCenter, 0.0f);
        } else {
            bounds = new UWBGL_BoundingList();
        }
        if (this.m_Primitive != null) {
            bounds.add(this.m_Primitive.getBoundingVolume(lod));
        }
        helper.pushModelTransform(gl);
        this.m_Xform.setUpModelStack(gl, helper);
        helper.transformBounds(gl, bounds);
        int count = this.m_ChildNodes.size();
        for (int i = 0; i < count; ++i) {
            bounds.add(this.m_ChildNodes.get(i).getBounds(gl, lod, helper, bDraw));
        }
        helper.popModelTransform(gl);
        return bounds;
    }

    public UWBGL_BoundingVolume getNodeBounds(GL gl, UWBGL_ELevelOfDetail lod, UWBGL_SceneNode pSearchNode, UWBGL_DrawHelper helper, boolean bDraw) {
        return this.getNodeBoundsHelper(gl, lod, pSearchNode, helper, 0, bDraw);
    }

    private UWBGL_BoundingVolume getNodeBoundsHelper(GL gl, UWBGL_ELevelOfDetail lod, UWBGL_SceneNode pSearchNode, UWBGL_DrawHelper helper, int level, boolean bDraw) {
        UWBGL_BoundingVolume bounds = null;
        if (this == pSearchNode) {
            bounds = this.getBounds(gl, lod, helper, bDraw);
        } else {
            UWBGL_SceneNode pChildNode;
            helper.pushModelTransform(gl);
            ++level;
            this.getXform().setUpModelStack(gl, helper);
            int count = this.numChildren();
            for (int i = 0; i < count && (bounds = (pChildNode = this.getChildNode(i)).getNodeBoundsHelper(gl, lod, pSearchNode, helper, level, bDraw)) == null; ++i) {
            }
            --level;
            helper.popModelTransform(gl);
        }
        if (0 == level && bounds != null && bDraw) {
            bounds.draw(gl, helper);
        }
        return bounds;
    }

    public UWBGL_BoundingVolume getNodeBounds(UWBGL_ELevelOfDetail lod, UWBGL_SceneNode pSearchNode, UWBGL_DrawHelper helper) {
        UWBGL_BoundingVolume bounds = null;
        if (this == pSearchNode) {
            bounds = this.getBounds(lod, helper);
        } else {
            UWBGL_SceneNode pChildNode;
            helper.pushModelTransform();
            helper.accumulateModelTransform(this.m_Xform);
            int count = this.numChildren();
            for (int i = 0; i < count && (bounds = (pChildNode = this.getChildNode(i)).getNodeBounds(lod, pSearchNode, helper)) == null; ++i) {
            }
            helper.popModelTransform();
        }
        return bounds;
    }

    public UWBGL_BoundingVolume getPrimitiveBounds(UWBGL_ELevelOfDetail lod, UWBGL_Primitive pSearchPrimitive, UWBGL_DrawHelper helper) {
        UWBGL_BoundingVolume bounds = null;
        helper.pushModelTransform();
        helper.accumulateModelTransform(this.m_Xform);
        if (this.m_Primitive != null && this.m_Primitive.hasPrimitive(pSearchPrimitive)) {
            bounds = pSearchPrimitive.getBoundingVolume(lod);
            helper.transformBounds(bounds);
        } else {
            UWBGL_SceneNode pChildNode;
            int count = this.numChildren();
            for (int i = 0; i < count && (bounds = (pChildNode = this.getChildNode(i)).getPrimitiveBounds(lod, pSearchPrimitive, helper)) == null; ++i) {
            }
        }
        helper.popModelTransform();
        return bounds;
    }

    public UWBGL_BoundingVolume getBounds(UWBGL_ELevelOfDetail lod, UWBGL_DrawHelper helper) {
        UWBGL_BoundingVolume bounds;
        if (lod == UWBGL_ELevelOfDetail.Low) {
            Vec3 boundsCenter = new Vec3();
            if (this.m_Primitive != null) {
                boundsCenter = this.m_Primitive.getLocation().clone();
            }
            bounds = new UWBGL_BoundingSphere(boundsCenter, 0.0f);
        } else {
            bounds = new UWBGL_BoundingList();
        }
        if (this.m_Primitive != null) {
            bounds.add(this.m_Primitive.getBoundingVolume(lod));
        }
        helper.pushModelTransform();
        helper.accumulateModelTransform(this.m_Xform);
        helper.transformBounds(bounds);
        int count = this.m_ChildNodes.size();
        for (int i = 0; i < count; ++i) {
            bounds.add(this.m_ChildNodes.get(i).getBounds(lod, helper));
        }
        helper.popModelTransform();
        return bounds;
    }

    public void setName(String name) {
        this.m_Name = name;
    }

    public String toString() {
        return this.m_Name;
    }
}

