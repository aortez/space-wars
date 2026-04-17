/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 */
import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingVolume;
import UWBGL_JavaOpenGL.UWBGL_DrawHelper;
import UWBGL_JavaOpenGL.UWBGL_ELevelOfDetail;
import UWBGL_JavaOpenGL.UWBGL_SceneNode;
import UWBGL_JavaOpenGL.math3d.Vec3;

public interface Entity
extends BasicEntity {
    public float getMass();

    public boolean intersects(UWBGL_SceneNode var1, Entity var2, UWBGL_DrawHelper var3);

    public void setCollided(boolean var1);

    public boolean collided();

    public void setMoveable(boolean var1);

    public void setCollideable(boolean var1);

    public void setFacingDirection(Vec3 var1);

    public Vec3 getFacingDirection();

    public void setVelocity(Vec3 var1);

    public void setOmega(float var1);

    public void translateVelocity(Vec3 var1);

    public Vec3 getVelocity();

    public boolean moveable();

    public boolean collideable();

    public void dirty();

    public boolean hasParent();

    public UWBGL_BoundingVolume getBounds(UWBGL_ELevelOfDetail var1, UWBGL_DrawHelper var2);

    public Munition getMunition();

    public float getLife();

    public void setLife(float var1);

    public void translateLife(float var1);

    public boolean dead();

    public void setDead(boolean var1);
}

