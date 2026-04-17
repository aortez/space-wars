/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 * 
 * Could not load the following classes:
 *  javax.media.opengl.GL
 */
import UWBGL_JavaOpenGL.UWBGL_DrawHelper;
import UWBGL_JavaOpenGL.UWBGL_ELevelOfDetail;
import UWBGL_JavaOpenGL.UWBGL_SceneNode;
import UWBGL_JavaOpenGL.math3d.Vec3;
import javax.media.opengl.GL;

public interface Munition {
    public Vec3 findIntersect(UWBGL_SceneNode var1, Entity var2, UWBGL_DrawHelper var3);

    public void fire(Vec3 var1, Vec3 var2, Vec3 var3);

    public void continueFiring(Vec3 var1, Vec3 var2);

    public void quitFiring();

    public boolean firing();

    public void collidedAt(Vec3 var1);

    public float damageAmount();

    public Debris update(float var1);

    public Vec3 getFacingDirection();

    public void draw(GL var1, UWBGL_ELevelOfDetail var2, UWBGL_DrawHelper var3);

    public boolean intersects(UWBGL_SceneNode var1, Entity var2, UWBGL_DrawHelper var3);
}

