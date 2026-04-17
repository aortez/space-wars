/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 */
package UWBGL_JavaOpenGL;

import UWBGL_JavaOpenGL.UWBGL_SceneNode;
import UWBGL_JavaOpenGL.UWBGL_SceneTreeModel;
import javax.swing.JTree;

public class UWBGL_SceneTree
extends JTree {
    public UWBGL_SceneTree() {
        super(new UWBGL_SceneTreeModel());
    }

    public void setRoot(UWBGL_SceneNode root) {
        this.setModel(new UWBGL_SceneTreeModel(root));
    }
}

