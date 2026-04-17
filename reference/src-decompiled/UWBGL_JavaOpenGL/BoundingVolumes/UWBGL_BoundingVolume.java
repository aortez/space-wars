/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 * 
 * Could not load the following classes:
 *  javax.media.opengl.GL
 */
package UWBGL_JavaOpenGL.BoundingVolumes;

import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_EVolumeType;
import UWBGL_JavaOpenGL.UWBGL_DrawHelper;
import UWBGL_JavaOpenGL.math3d.Vec3;
import javax.media.opengl.GL;

public interface UWBGL_BoundingVolume {
    public UWBGL_EVolumeType getType();

    public Vec3 getCenter();

    public void add(UWBGL_BoundingVolume var1);

    public boolean intersects(UWBGL_BoundingVolume var1);

    public Vec3 isContainedBy(Vec3 var1, Vec3 var2);

    public Vec3 isContainedBy(Vec3 var1, float var2);

    public boolean containsPoint(Vec3 var1);

    public void draw(GL var1, UWBGL_DrawHelper var2);
}

