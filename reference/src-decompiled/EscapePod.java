/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 */
import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingSphere;
import UWBGL_JavaOpenGL.Primitives.UWBGL_Primitive;
import UWBGL_JavaOpenGL.Primitives.UWBGL_PrimitiveCircle;
import UWBGL_JavaOpenGL.Primitives.UWBGL_PrimitiveList;
import UWBGL_JavaOpenGL.Primitives.UWBGL_PrimitiveTriangle;
import UWBGL_JavaOpenGL.UWBGL_Color;
import UWBGL_JavaOpenGL.UWBGL_EFillMode;
import UWBGL_JavaOpenGL.UWBGL_ELevelOfDetail;
import UWBGL_JavaOpenGL.UWBGL_EShadeMode;
import UWBGL_JavaOpenGL.math3d.Vec3;
import java.util.ArrayList;

public class EscapePod
extends Ship
implements Mount {
    static final float POD_TURN_EXHAUST_SCALAR = 15.0f;
    UWBGL_PrimitiveCircle cockpit;
    boolean m_change_back_to_ship = false;

    EscapePod(int playerId, Vec3 pos, float thrust, float turn, UWBGL_Color base_color, boolean useTextures, float delta_time) {
        super(playerId, pos, thrust, turn, 100, base_color, useTextures, delta_time);
        this.MAX_SPEED = 500.0f;
        this.WING_CLOSED_SPEED = 3.0f * this.MAX_SPEED;
        this.cannon = null;
        this.setupBody(base_color, useTextures);
        this.m_Mass = this.body_tri.getSize();
        this.thrust_force = thrust;
        this.turn_force = turn;
        this.m_Direction = new Vec3(0.0f, 1.0f);
        this.updatePower(delta_time);
        this.calcRadius();
    }

    @Override
    protected void setupBody(UWBGL_Color base_color, boolean useTextures) {
        this.left_wing = null;
        this.right_wing = null;
        float z = -0.8f;
        this.body_tri = new UWBGL_PrimitiveTriangle(new Vec3(0.0f, 1.0f, z), new Vec3(-1.0f, 0.0f, z), new Vec3(1.0f, 0.0f, z));
        this.setPrimitive(this.body_tri);
        this.body_tri.setFlatColor(base_color);
        this.body_tri.setFillMode(UWBGL_EFillMode.Solid);
        if (useTextures) {
            this.body_tri.setTextureFileName("./rec/texturedMetal.jpg");
            this.body_tri.setTexturing(true);
        }
        this.getXform().setPivot(this.body_tri.getCenter());
        Vec3 cockpit_loc = this.body_tri.getCenter();
        cockpit_loc.setZ(z - 0.02f);
        this.cockpit = new UWBGL_PrimitiveCircle(cockpit_loc, 0.5f);
        this.cockpit.setFlatColor(UWBGL_Color.BLUE);
        this.cockpit.setShadeMode(UWBGL_EShadeMode.Gouraud);
        this.cockpit.setFillMode(UWBGL_EFillMode.Solid);
        z = -0.81f;
        this.thruster = new UWBGL_PrimitiveTriangle(new Vec3(0.0f, 1.1f, z), new Vec3(-1.0f, 0.0f, z), new Vec3(1.0f, 0.0f, z));
        this.thruster.setFlatColor(base_color.randomVariation(0.1f));
        this.thruster.setFillMode(UWBGL_EFillMode.Solid);
        UWBGL_PrimitiveList part_list = new UWBGL_PrimitiveList();
        part_list.add(this.thruster);
        part_list.add(this.cockpit);
        this.thruster_node.setPrimitive(part_list);
        this.insertChildNode(this.thruster_node);
        z = -0.1f;
        this.laser_triangle = new UWBGL_PrimitiveTriangle(new Vec3(-0.5f, 0.5f, z), new Vec3(0.0f, 2.5f, z), new Vec3(0.5f, 0.5f, z));
        this.laser_triangle.setFlatColor(UWBGL_Color.BLUE);
        this.laser_triangle.setFillMode(UWBGL_EFillMode.Solid);
        this.laser_node.setPrimitive(this.laser_triangle);
        this.insertChildNode(this.laser_node);
    }

    @Override
    protected void fireExhaust(Vec3 direction) {
        Vec3 init_pos = this.getNodeBounds(UWBGL_ELevelOfDetail.Low, this.thruster_node, DRAW_HELPER).getCenter();
        direction = Vec3.normFromRadians(this.getXform().getRotationInRadians());
        this.exhaust.add(new ExhaustTrail(init_pos, direction.times(this.THRUST_POWER * -0.5f * ((float)Math.cos(System.currentTimeMillis() % 614L / 1000L) + 1.5707964f) * 0.001f).rotateR(-0.01f), 0.01f));
        this.exhaust.add(new ExhaustTrail(init_pos, direction.times(this.THRUST_POWER * -0.5f * ((float)Math.cos(System.currentTimeMillis() % 614L / 1000L) + 1.5707964f) * 0.001f).rotateR(0.01f), 0.01f));
    }

    @Override
    void fireCannonHalt() {
    }

    @Override
    protected void updateWingPos() {
    }

    @Override
    public Munition getMunition() {
        if (this.laser.firing()) {
            return this.laser;
        }
        return null;
    }

    private void calcRadius() {
        this.radius = ((UWBGL_BoundingSphere)this.getBounds(UWBGL_ELevelOfDetail.Low, DRAW_HELPER)).getRadius();
    }

    @Override
    public ArrayList<UWBGL_Primitive> getPrimimatives() {
        ArrayList<UWBGL_Primitive> primitives = new ArrayList<UWBGL_Primitive>();
        primitives.add(this.laser_triangle);
        primitives.add(this.cockpit);
        primitives.add(this.thruster);
        primitives.add(this.body_tri);
        return primitives;
    }

    @Override
    protected void fireCannon() {
    }

    @Override
    protected void fireLaser() {
    }

    @Override
    public void update(float delta_t) {
        this.m_Velocity.timesEquals(0.8f);
        super.update(delta_t);
    }

    @Override
    public void translateLife(float life) {
    }

    @Override
    public boolean dead() {
        return false;
    }

    public boolean changeToShip() {
        return this.m_change_back_to_ship;
    }

    public void setChangeToShip(boolean c) {
        this.m_change_back_to_ship = c;
    }
}

