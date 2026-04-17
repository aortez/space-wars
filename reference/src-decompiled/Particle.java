/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 * 
 * Could not load the following classes:
 *  javax.media.opengl.GL
 */
import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingSphere;
import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingVolume;
import UWBGL_JavaOpenGL.Primitives.UWBGL_Primitive;
import UWBGL_JavaOpenGL.UWBGL_Color;
import UWBGL_JavaOpenGL.UWBGL_DrawHelper;
import UWBGL_JavaOpenGL.UWBGL_ELevelOfDetail;
import UWBGL_JavaOpenGL.UWBGL_SceneNode;
import UWBGL_JavaOpenGL.UWBGL_Xform;
import UWBGL_JavaOpenGL.math3d.Vec3;
import javax.media.opengl.GL;

public class Particle
implements BasicEntity,
ObeysForces {
    private UWBGL_Color m_color = null;
    private Vec3 m_Velocity = new Vec3();
    private UWBGL_Primitive m_primitive = null;
    private boolean m_Visible = true;
    private float m_Fade_Rate = 0.19607843f;
    private UWBGL_BoundingVolume m_bounds;
    private boolean dirty = true;
    private float m_radius_cache;
    private Vec3 m_loc_cache;

    Particle(Vec3 velocity, UWBGL_Color color, UWBGL_Primitive primitive, float fade) {
        this.m_color = new UWBGL_Color(color);
        this.m_Velocity.set(velocity);
        this.m_primitive = primitive;
        this.m_primitive.setFlatColor(color);
        this.m_Fade_Rate = fade;
        this.clean();
    }

    @Override
    public UWBGL_Xform getXform() {
        throw new UnsupportedOperationException("Not supported yet.");
    }

    public boolean intersects(UWBGL_SceneNode root, Entity other, UWBGL_DrawHelper helper) {
        UWBGL_BoundingVolume that_low = null;
        UWBGL_BoundingVolume this_low = this.getBounds();
        if (this_low.intersects(that_low = other.hasParent() ? other.getBounds(UWBGL_ELevelOfDetail.Low, helper) : other.getBounds(UWBGL_ELevelOfDetail.Low, helper))) {
            UWBGL_BoundingVolume that_high = null;
            that_high = other.hasParent() ? root.getNodeBounds(UWBGL_ELevelOfDetail.High, (UWBGL_SceneNode)((Object)other), helper) : other.getBounds(UWBGL_ELevelOfDetail.High, helper);
            return this_low.intersects(that_high);
        }
        return false;
    }

    public UWBGL_BoundingVolume getBounds() {
        if (this.dirty) {
            this.clean();
        }
        return this.m_bounds;
    }

    @Override
    public UWBGL_BoundingVolume getBounds(UWBGL_ELevelOfDetail lod, UWBGL_DrawHelper helper) {
        return this.getBounds();
    }

    private void calcLocation() {
        this.m_loc_cache = this.m_bounds.getCenter();
    }

    private void calcRadius() {
        this.m_radius_cache = ((UWBGL_BoundingSphere)this.getBounds()).getRadius();
    }

    private void calcBounds() {
        this.m_bounds = this.m_primitive.getBoundingVolume(UWBGL_ELevelOfDetail.Low);
    }

    @Override
    public void draw(GL gl, UWBGL_ELevelOfDetail lod, UWBGL_DrawHelper helper) {
        this.m_primitive.draw(gl, lod, helper);
    }

    @Override
    public void update(float time_delta) {
        this.m_primitive.moveBy(this.m_Velocity.times(time_delta));
        this.m_color.red -= this.m_Fade_Rate * time_delta;
        this.m_color.blue -= this.m_Fade_Rate * time_delta;
        this.m_color.green -= this.m_Fade_Rate * time_delta;
        this.m_primitive.setFlatColor(this.m_color);
        this.dirty();
    }

    public boolean done() {
        return this.m_color.red <= 0.1f && this.m_color.blue <= 0.1f && this.m_color.green <= 0.1f;
    }

    @Override
    public boolean visible() {
        return this.m_Visible;
    }

    @Override
    public void setVisible(boolean v) {
        this.m_Visible = v;
    }

    @Override
    public float getMass() {
        return 0.1f;
    }

    @Override
    public Vec3 getLocation() {
        if (this.dirty) {
            this.clean();
        }
        return this.m_loc_cache;
    }

    @Override
    public void applyForce(float force, Vec3 angle) {
        this.m_Velocity.plusEquals(angle.timesEquals(force / 0.1f));
    }

    @Override
    public float getRadius() {
        if (this.dirty) {
            this.clean();
        }
        return this.m_radius_cache;
    }

    public void clean() {
        this.calcBounds();
        this.dirty = false;
        this.calcRadius();
        this.calcLocation();
    }

    @Override
    public void dirty() {
        this.dirty = true;
    }

    @Override
    public void setTranslation(Vec3 t) {
        this.m_primitive.moveTo(t);
    }

    @Override
    public void setVelocity(Vec3 v) {
        this.m_Velocity = v;
    }

    @Override
    public Vec3 getVelocity() {
        return this.m_Velocity;
    }

    @Override
    public float getLife() {
        return 0.0f;
    }

    @Override
    public void setLife(float life) {
    }

    @Override
    public void translateLife(float life) {
    }

    @Override
    public boolean dead() {
        return false;
    }

    @Override
    public UWBGL_Color getColor() {
        return this.m_color;
    }
}

