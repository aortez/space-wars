/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 * 
 * Could not load the following classes:
 *  javax.media.opengl.GL
 */
import UWBGL_JavaOpenGL.Primitives.UWBGL_PrimitivePolygon;
import UWBGL_JavaOpenGL.UWBGL_Color;
import UWBGL_JavaOpenGL.UWBGL_DrawHelper;
import UWBGL_JavaOpenGL.UWBGL_ELevelOfDetail;
import UWBGL_JavaOpenGL.UWBGL_Xform;
import UWBGL_JavaOpenGL.math3d.Vec3;
import javax.media.opengl.GL;

public class ExhaustTrail
implements BasicEntity {
    private float decay = 0.0f;
    private Vec3 velocity;
    private UWBGL_PrimitivePolygon shape;
    private UWBGL_Color color = UWBGL_Color.scale255(255.0f, (float)Math.random() * 50.0f, (float)Math.random() * 50.0f, 0.0f);

    ExhaustTrail(Vec3 loc, Vec3 velocity, float decay_rate) {
        Vec3[] points = new Vec3[]{new Vec3(loc), new Vec3(loc.plus(velocity.times(0.025f)))};
        this.shape = new UWBGL_PrimitivePolygon(points);
        this.shape.setFlatColor(this.color);
        this.velocity = velocity;
        this.decay = decay_rate;
    }

    @Override
    public UWBGL_Xform getXform() {
        throw new UnsupportedOperationException("Not supported yet.");
    }

    @Override
    public void draw(GL gl, UWBGL_ELevelOfDetail lod, UWBGL_DrawHelper helper) {
        this.shape.draw(gl, lod, helper);
    }

    @Override
    public void update(float time_delta) {
        this.shape.setCenter(this.shape.getCenter().plus(this.velocity.times(time_delta * 0.01f)));
        this.color.blue -= this.decay;
        this.color.green -= this.decay;
        this.color.red -= this.decay;
        if (this.color.blue < 0.0f) {
            this.color.blue = 0.0f;
        }
        if (this.color.red < 0.0f) {
            this.color.red = 0.0f;
        }
        if (this.color.green < 0.0f) {
            this.color.green = 0.0f;
        }
        this.shape.setFlatColor(this.color);
        this.shape.setSize(this.shape.getSize() * (float)Math.sin(this.color.red) * (float)Math.cos(this.color.red));
    }

    public boolean done() {
        return this.color.blue == 0.0f && this.color.red == 0.0f && this.color.green == 0.0f;
    }

    @Override
    public boolean visible() {
        throw new UnsupportedOperationException("Not supported yet.");
    }

    @Override
    public void setVisible(boolean v) {
        throw new UnsupportedOperationException("Not supported yet.");
    }

    @Override
    public Vec3 getLocation() {
        throw new UnsupportedOperationException("Not supported yet.");
    }

    @Override
    public UWBGL_Color getColor() {
        return this.color;
    }
}

