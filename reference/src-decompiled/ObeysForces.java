/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 */
import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingVolume;
import UWBGL_JavaOpenGL.UWBGL_Color;
import UWBGL_JavaOpenGL.UWBGL_DrawHelper;
import UWBGL_JavaOpenGL.UWBGL_ELevelOfDetail;
import UWBGL_JavaOpenGL.math3d.Vec3;

public interface ObeysForces
extends HasLife {
    public void dirty();

    public void setTranslation(Vec3 var1);

    public UWBGL_Color getColor();

    public void setVelocity(Vec3 var1);

    public float getMass();

    public Vec3 getLocation();

    public float getRadius();

    public void applyForce(float var1, Vec3 var2);

    @Override
    public Vec3 getVelocity();

    public UWBGL_BoundingVolume getBounds(UWBGL_ELevelOfDetail var1, UWBGL_DrawHelper var2);
}

