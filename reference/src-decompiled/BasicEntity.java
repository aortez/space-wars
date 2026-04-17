/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 * 
 * Could not load the following classes:
 *  javax.media.opengl.GL
 */
import UWBGL_JavaOpenGL.UWBGL_Color;
import UWBGL_JavaOpenGL.UWBGL_DrawHelper;
import UWBGL_JavaOpenGL.UWBGL_ELevelOfDetail;
import UWBGL_JavaOpenGL.UWBGL_Xform;
import UWBGL_JavaOpenGL.math3d.Vec3;
import javax.media.opengl.GL;

public interface BasicEntity
extends Common {
    public UWBGL_Color getColor();

    public Vec3 getLocation();

    public UWBGL_Xform getXform();

    public void draw(GL var1, UWBGL_ELevelOfDetail var2, UWBGL_DrawHelper var3);

    public void update(float var1);

    public boolean visible();

    public void setVisible(boolean var1);
}

