/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 */
import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingVolume;
import UWBGL_JavaOpenGL.UWBGL_DrawHelper;
import UWBGL_JavaOpenGL.UWBGL_ELevelOfDetail;
import UWBGL_JavaOpenGL.UWBGL_SceneNode;
import UWBGL_JavaOpenGL.math3d.Vec3;

public abstract class AbstractEntity
extends UWBGL_SceneNode
implements Entity,
Common,
ObeysForces,
HasLife {
    protected Vec3 m_Direction;
    protected boolean m_Visible = true;
    protected boolean m_Collideable = true;
    protected boolean m_Moveable = true;
    protected float m_Mass = 0.0f;
    protected float m_Omega = 0.0f;
    private boolean m_low_bound_dirty = true;
    private boolean m_high_bound_dirty = true;
    UWBGL_BoundingVolume m_low_bound;
    UWBGL_BoundingVolume m_high_bound;
    protected boolean m_HasParent = false;
    private boolean m_collided = false;
    protected float m_life;
    protected float m_lifeMax = this.m_life = 100.0f;
    protected boolean m_dead = false;

    AbstractEntity(String name) {
        super(name);
        this.m_Velocity = new Vec3();
    }

    AbstractEntity(String name, Vec3 loc) {
        this(name);
        this.getXform().setTranslation(loc);
    }

    @Override
    public void setOmega(float o) {
        this.m_Omega = o;
    }

    @Override
    public void update(float delta_t) {
        this.dirty();
        this.getXform().translateTranslation(this.m_Velocity.times(delta_t));
        this.getXform().translateRotation(this.m_Omega * delta_t);
    }

    @Override
    public boolean collided() {
        return this.m_collided;
    }

    @Override
    public void setCollided(boolean c) {
        this.m_collided = c;
    }

    @Override
    public void applyForce(float force, Vec3 angle) {
        this.m_Velocity.plusEquals(angle.timesEquals(force / this.m_Mass));
    }

    @Override
    public UWBGL_BoundingVolume getBounds(UWBGL_ELevelOfDetail lod, UWBGL_DrawHelper helper) {
        switch (lod) {
            case Low: {
                if (this.m_low_bound_dirty) {
                    this.m_low_bound = super.getBounds(UWBGL_ELevelOfDetail.Low, helper);
                    this.m_low_bound_dirty = false;
                }
                return this.m_low_bound;
            }
        }
        if (this.m_high_bound_dirty) {
            this.m_high_bound = super.getBounds(UWBGL_ELevelOfDetail.High, helper);
            this.m_high_bound_dirty = false;
        }
        return this.m_high_bound;
    }

    @Override
    public boolean intersects(UWBGL_SceneNode root, Entity other, UWBGL_DrawHelper helper) {
        UWBGL_BoundingVolume this_low = null;
        UWBGL_BoundingVolume that_low = null;
        this_low = this.hasParent() ? root.getNodeBounds(UWBGL_ELevelOfDetail.Low, this, helper) : this.getBounds(UWBGL_ELevelOfDetail.Low, helper);
        if (this_low.intersects(that_low = other.hasParent() ? root.getNodeBounds(UWBGL_ELevelOfDetail.Low, (UWBGL_SceneNode)((Object)other), helper) : other.getBounds(UWBGL_ELevelOfDetail.Low, helper))) {
            UWBGL_BoundingVolume this_high = null;
            this_high = this.hasParent() ? root.getNodeBounds(UWBGL_ELevelOfDetail.High, this, helper) : this.getBounds(UWBGL_ELevelOfDetail.High, helper);
            UWBGL_BoundingVolume that_high = null;
            that_high = other.hasParent() ? root.getNodeBounds(UWBGL_ELevelOfDetail.High, (UWBGL_SceneNode)((Object)other), helper) : other.getBounds(UWBGL_ELevelOfDetail.High, helper);
            return this_high.intersects(that_high);
        }
        return false;
    }

    @Override
    public Vec3 getFacingDirection() {
        return Vec3.normFromRadians(this.getXform().getRotationInRadians());
    }

    @Override
    public void setVisible(boolean v) {
        this.m_Visible = v;
    }

    @Override
    public void setMoveable(boolean m) {
        this.m_Moveable = m;
    }

    @Override
    public void setCollideable(boolean c) {
        this.m_Collideable = c;
    }

    @Override
    public boolean visible() {
        return this.m_Visible;
    }

    @Override
    public boolean moveable() {
        return this.m_Moveable;
    }

    @Override
    public boolean collideable() {
        return this.m_Collideable;
    }

    @Override
    public Vec3 getVelocity() {
        return this.m_Velocity;
    }

    @Override
    public float getMass() {
        return this.m_Mass;
    }

    @Override
    public boolean hasParent() {
        return this.m_HasParent;
    }

    @Override
    public void dirty() {
        this.m_low_bound_dirty = true;
        this.m_high_bound_dirty = true;
    }

    @Override
    public float getLife() {
        return this.m_life;
    }

    public float getLifeMax() {
        return this.m_lifeMax;
    }

    @Override
    public boolean dead() {
        if (!this.m_dead && this.m_life <= 0.0f) {
            this.m_dead = true;
        }
        return this.m_dead;
    }

    @Override
    public void setDead(boolean dead) {
        this.m_dead = dead;
    }

    @Override
    public void setLife(float life) {
        this.m_life = life;
    }

    @Override
    public void translateLife(float life) {
        this.m_life += life;
    }

    @Override
    public Munition getMunition() {
        return null;
    }

    @Override
    public Vec3 getLocation() {
        return this.getXform().getTranslation();
    }

    @Override
    public void setTranslation(Vec3 t) {
        this.getXform().setTranslation(t);
    }
}

