/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 */
package UWBGL_JavaOpenGL;

import UWBGL_JavaOpenGL.UWBGL_SceneNode;
import java.util.ArrayList;
import java.util.List;
import javax.swing.event.TreeModelListener;
import javax.swing.tree.TreeModel;
import javax.swing.tree.TreePath;

class UWBGL_SceneTreeModel
implements TreeModel {
    private UWBGL_SceneNode m_Root = null;
    private List<TreeModelListener> m_TreeListeners = new ArrayList<TreeModelListener>();

    public UWBGL_SceneTreeModel() {
    }

    public UWBGL_SceneTreeModel(UWBGL_SceneNode root) {
        this();
        this.m_Root = root;
    }

    @Override
    public Object getRoot() {
        return this.m_Root;
    }

    private UWBGL_SceneNode cast(Object obj) {
        return (UWBGL_SceneNode)obj;
    }

    @Override
    public Object getChild(Object parent, int index) {
        return this.cast(parent).getChildNode(index);
    }

    @Override
    public int getChildCount(Object parent) {
        return this.cast(parent).numChildren();
    }

    @Override
    public boolean isLeaf(Object node) {
        return this.getChildCount(node) <= 0;
    }

    @Override
    public void valueForPathChanged(TreePath path, Object newValue) {
        throw new UnsupportedOperationException("Not supported yet.");
    }

    @Override
    public int getIndexOfChild(Object parent, Object child) {
        throw new UnsupportedOperationException("Not supported yet.");
    }

    @Override
    public void addTreeModelListener(TreeModelListener l) {
        this.m_TreeListeners.add(l);
    }

    @Override
    public void removeTreeModelListener(TreeModelListener l) {
        this.m_TreeListeners.remove(l);
    }
}

