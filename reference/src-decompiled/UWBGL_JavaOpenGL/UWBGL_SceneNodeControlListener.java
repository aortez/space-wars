/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 */
package UWBGL_JavaOpenGL;

import UWBGL_JavaOpenGL.UWBGL_SceneNode;

public interface UWBGL_SceneNodeControlListener {
    public static final int UP = 1;
    public static final int DOWN = -1;
    public static final int LEFT = -1;
    public static final int RIGHT = 1;

    public void setRotationNode(UWBGL_SceneNode var1, float var2);

    public void rotateNode(UWBGL_SceneNode var1, int var2);

    public void pivotNodeX(UWBGL_SceneNode var1, int var2);

    public void pivotNodeY(UWBGL_SceneNode var1, int var2);

    public void translateNodeX(UWBGL_SceneNode var1, int var2);

    public void translateNodeY(UWBGL_SceneNode var1, int var2);

    public void scaleNodeX(UWBGL_SceneNode var1, int var2);

    public void scaleNodeY(UWBGL_SceneNode var1, int var2);

    public void setShowPivot(UWBGL_SceneNode var1, boolean var2);
}

