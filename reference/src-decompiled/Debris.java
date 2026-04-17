/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 */
import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingSphere;
import UWBGL_JavaOpenGL.Primitives.UWBGL_Primitive;
import UWBGL_JavaOpenGL.UWBGL_Color;
import UWBGL_JavaOpenGL.UWBGL_EFillMode;
import UWBGL_JavaOpenGL.UWBGL_ELevelOfDetail;
import UWBGL_JavaOpenGL.math3d.Vec3;

public class Debris
extends AbstractEntity
implements CausesDamage {
    private UWBGL_Primitive body;
    private float m_radius;
    private float m_damage = 0.0f;

    Debris(Vec3 loc, UWBGL_Primitive shape, Vec3 velocity, boolean useTextures) {
        super("Asteroid", loc);
        this.body = shape;
        if (useTextures) {
            this.body.setTextureFileName("./rec/asteroid.gif");
            this.body.setTexturing(true);
        }
        this.m_Velocity = velocity;
        UWBGL_Color base_color = UWBGL_Color.DIM_GREY;
        this.body.setFlatColor(base_color.randomVariation(0.2f));
        this.body.setFillMode(UWBGL_EFillMode.Solid);
        this.setPrimitive(this.body);
        this.getXform().setPivot(this.body.getLocation());
        this.m_radius = ((UWBGL_BoundingSphere)this.body.getBoundingVolume(UWBGL_ELevelOfDetail.Low)).getRadius();
        this.calcMass();
        this.setLife(this.m_Mass * 0.5f);
        this.m_lifeMax = this.getLife();
    }

    Debris(Vec3 loc, UWBGL_Primitive shape, Vec3 velocity, float damage, boolean useTextures) {
        this(loc, shape, velocity, useTextures);
        this.m_damage = damage;
    }

    @Override
    public void update(float delta_t) {
        this.rotate(delta_t);
        super.update(delta_t);
    }

    @Override
    public void setFacingDirection(Vec3 dir) {
        throw new UnsupportedOperationException("Not supported yet.");
    }

    @Override
    public float getMass() {
        return this.getArea();
    }

    public float getArea() {
        return (float)Math.PI * 2 * this.m_radius;
    }

    private void rotate(float delta_t) {
        this.getXform().translateRotation(this.m_Omega * delta_t);
    }

    @Override
    public void setOmega(float o) {
        this.m_Omega = o;
    }

    @Override
    public String toString() {
        return "location: " + this.getXform().getTranslation() + ", radius: " + this.m_radius + ", life: " + this.getLife();
    }

    @Override
    public UWBGL_Color getColor() {
        return this.body.getFlatColor();
    }

    @Override
    public float getRadius() {
        return this.m_radius;
    }

    public void calcMass() {
        this.m_Mass = this.getArea();
    }

    @Override
    public void translateLife(float delta_life) {
        super.translateLife(delta_life);
        this.updateSize();
    }

    private void updateSize() {
        float factor = this.getLife() / this.m_lifeMax;
        if ((double)factor < 0.8) {
            this.setLife(0.0f);
            this.shrinkTo(0.01f);
        } else {
            this.shrinkTo(factor);
        }
    }

    public void shrinkTo(float percent) {
        this.m_radius *= percent;
        this.body.setSize(this.m_radius);
        this.calcMass();
    }

    @Override
    public float damageAmount(Vec3 relative_velocity) {
        float damage = this.m_damage * relative_velocity.minus(this.m_Velocity).length();
        return damage;
    }
}

