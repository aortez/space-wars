/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 * 
 * Could not load the following classes:
 *  javax.media.opengl.GL
 */
import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingSphere;
import UWBGL_JavaOpenGL.Primitives.UWBGL_Primitive;
import UWBGL_JavaOpenGL.Primitives.UWBGL_PrimitiveList;
import UWBGL_JavaOpenGL.Primitives.UWBGL_PrimitiveTriangle;
import UWBGL_JavaOpenGL.UWBGL_Color;
import UWBGL_JavaOpenGL.UWBGL_DrawHelper;
import UWBGL_JavaOpenGL.UWBGL_EFillMode;
import UWBGL_JavaOpenGL.UWBGL_ELevelOfDetail;
import UWBGL_JavaOpenGL.UWBGL_SceneNode;
import UWBGL_JavaOpenGL.math3d.Vec3;
import java.util.ArrayList;
import javax.media.opengl.GL;

public class Ship
extends AbstractEntity
implements Common,
Mount {
    protected static final float SHIP_TURN_EXHAUST_SCALAR = 50.0f;
    protected static final float MAX_WING_THETA = 0.7853982f;
    protected float BASE_MAX_OMEGA = 1.0f;
    protected float MAX_SPEED = 150.0f;
    protected final float WING_DELTA_SPEED = 5.0f;
    protected float WING_CLOSED_SPEED = this.MAX_SPEED * 5.0f;
    protected final float WING_CLOSED_MAX_OMEGA = this.BASE_MAX_OMEGA * 0.25f;
    protected Munition laser = new Laser(this);
    protected Munition cannon = new Cannon(this);
    protected float cur_max_omega = this.BASE_MAX_OMEGA;
    protected UWBGL_SceneNode laser_node = new UWBGL_SceneNode("Laser");
    protected UWBGL_SceneNode thruster_node = new UWBGL_SceneNode("Thruster");
    private UWBGL_SceneNode wing_mount_node = new UWBGL_SceneNode("Wing_Mount");
    private UWBGL_SceneNode left_wing_node = new UWBGL_SceneNode("Left_Wing");
    private UWBGL_SceneNode right_wing_node = new UWBGL_SceneNode("Right_Wing");
    protected Wing_Behavior wing_behavior = Wing_Behavior.none;
    protected Wing_State wing_state = Wing_State.opened;
    protected Thrust_Behavior thrust_behavior = Thrust_Behavior.none;
    protected Turn_Behavior turn_behavior = Turn_Behavior.none;
    protected Weapon_State weapon_state = Weapon_State.none;
    protected Weapon_Behavior weapon_behavior = Weapon_Behavior.none;
    protected float wing_theta = 0.0f;
    protected float thrust_force;
    protected float turn_force;
    protected float TURN_POWER;
    protected float THRUST_POWER;
    protected float radius;
    protected boolean WING_TOGGLE_IN_PROGRESS = false;
    protected UWBGL_PrimitiveTriangle laser_triangle;
    protected UWBGL_PrimitiveTriangle thruster;
    protected UWBGL_PrimitiveTriangle left_wing;
    protected UWBGL_PrimitiveTriangle right_wing;
    protected UWBGL_PrimitiveTriangle body_tri;
    protected ArrayList<ExhaustTrail> exhaust = new ArrayList(40);
    protected int m_PlayerId;
    protected UWBGL_Color m_BaseColor;
    static final float PI_OVER_2 = 1.5707964f;

    protected void wingsIn() {
        this.wing_behavior = Wing_Behavior.close;
    }

    protected void wingsOut() {
        this.wing_behavior = Wing_Behavior.open;
    }

    Ship(int playerId, Vec3 pos, float thrust, float turn, int healthMultiplier, UWBGL_Color base_color, boolean useTextures, float delta_time) {
        super("Ship", pos);
        this.m_PlayerId = playerId;
        this.m_BaseColor = base_color;
        this.m_life = (float)((double)this.m_life * ((double)healthMultiplier / 100.0));
        this.m_lifeMax = (float)((double)this.m_lifeMax * ((double)healthMultiplier / 100.0));
        this.setupBody(base_color, useTextures);
        this.thrust_force = thrust;
        this.turn_force = turn;
        this.m_Direction = new Vec3(0.0f, 1.0f);
        this.updatePower(delta_time);
        this.calcRadius();
    }

    protected void updatePower(float delta_time) {
        this.TURN_POWER = this.turn_force / this.m_Mass * delta_time;
        this.THRUST_POWER = this.thrust_force / this.m_Mass * delta_time;
    }

    protected void setupBody(UWBGL_Color base_color, boolean useTextures) {
        float z = -0.8f;
        this.body_tri = new UWBGL_PrimitiveTriangle(new Vec3(0.0f, 0.0f, z), new Vec3(5.0f, 0.0f, z), new Vec3(2.5f, 7.0f, z));
        this.setPrimitive(this.body_tri);
        this.body_tri.setFlatColor(base_color);
        this.body_tri.setFillMode(UWBGL_EFillMode.Solid);
        if (useTextures) {
            this.body_tri.setTextureFileName("./rec/texturedMetal.jpg");
            this.body_tri.setTexturing(true);
        }
        this.getXform().setPivot(this.body_tri.getCenter());
        z = -0.81f;
        Vec3 tri_p1 = new Vec3((float)Math.cos(1.5707963267948966) * 2.0f, (float)Math.sin(1.5707963267948966) * 2.0f, z).plus(this.body_tri.getCenter());
        Vec3 tri_p2 = new Vec3((float)Math.cos(3.665191429188092) * 2.0f, (float)Math.sin(3.665191429188092) * 2.0f, z).plus(this.body_tri.getCenter());
        tri_p1.setZ(z);
        UWBGL_PrimitiveTriangle wing_mount_tri = new UWBGL_PrimitiveTriangle(tri_p1, tri_p2, new Vec3((float)Math.cos(5.759586531581287) * 2.0f, (float)Math.sin(5.759586531581287) * 2.0f, z).plus(this.body_tri.getCenter()));
        wing_mount_tri.setFlatColor(UWBGL_Color.scale255(10.0f, 180.0f, 50.0f));
        wing_mount_tri.setFillMode(UWBGL_EFillMode.Solid);
        if (useTextures) {
            wing_mount_tri.setTextureFileName("./rec/green_glass.jpg");
            wing_mount_tri.setTexturing(true);
        }
        UWBGL_PrimitiveList wing_mount_list = new UWBGL_PrimitiveList();
        wing_mount_list.add(wing_mount_tri);
        this.wing_mount_node.setPrimitive(wing_mount_list);
        this.insertChildNode(this.wing_mount_node);
        this.wing_mount_node.getXform().setPivot(this.body_tri.getCenter());
        z = 0.0f;
        this.thruster = new UWBGL_PrimitiveTriangle(new Vec3(0.0f, -1.0f, z), new Vec3(2.5f, 0.0f, z), new Vec3(5.0f, -1.0f, z));
        this.thruster.setFlatColor(base_color.randomVariation(0.1f));
        this.thruster.setFillMode(UWBGL_EFillMode.Solid);
        this.thruster_node.setPrimitive(this.thruster);
        this.insertChildNode(this.thruster_node);
        z = -0.81f;
        this.laser_triangle = new UWBGL_PrimitiveTriangle(new Vec3(2.0f, 6.0f, z), new Vec3(2.5f, 7.0f, z), new Vec3(3.0f, 6.0f, z));
        this.laser_triangle.setFlatColor(base_color);
        this.laser_triangle.setFillMode(UWBGL_EFillMode.Solid);
        this.laser_node.setPrimitive(this.laser_triangle);
        this.insertChildNode(this.laser_node);
        z = -0.8f;
        this.left_wing = new UWBGL_PrimitiveTriangle(new Vec3(2.5f, 2.0f, z), new Vec3(1.0f, -0.5f, z), new Vec3(-3.0f, 2.0f, z));
        this.left_wing.setFlatColor(base_color.randomVariation(0.1f));
        this.left_wing.setFillMode(UWBGL_EFillMode.Solid);
        if (useTextures) {
            this.left_wing.setTextureFileName("./rec/texturedMetal.jpg");
            this.left_wing.setTexturing(true);
        }
        this.left_wing_node.setPrimitive(this.left_wing);
        this.wing_mount_node.insertChildNode(this.left_wing_node);
        this.left_wing_node.getXform().setPivot(new Vec3(2.5f, 2.0f, z));
        z = -0.8f;
        this.right_wing = new UWBGL_PrimitiveTriangle(new Vec3(2.5f, 2.0f, z), new Vec3(4.0f, -0.5f, z), new Vec3(8.0f, 2.0f, z));
        this.right_wing.setFlatColor(base_color.randomVariation(0.1f));
        this.right_wing.setFillMode(UWBGL_EFillMode.Solid);
        if (useTextures) {
            this.right_wing.setTextureFileName("./rec/texturedMetal.jpg");
            this.right_wing.setTexturing(true);
        }
        this.right_wing_node.setPrimitive(this.right_wing);
        this.wing_mount_node.insertChildNode(this.right_wing_node);
        this.right_wing_node.getXform().setPivot(new Vec3(2.5f, 2.0f, z));
        this.m_Mass = this.body_tri.getSize() + this.right_wing.getSize() + this.left_wing.getSize();
    }

    public int getOwnerId() {
        return this.m_PlayerId;
    }

    protected void toggleWings() {
        this.WING_TOGGLE_IN_PROGRESS = true;
        block0 : switch (this.wing_behavior) {
            case close: {
                this.wing_behavior = Wing_Behavior.open;
                break;
            }
            case open: {
                this.wing_behavior = Wing_Behavior.close;
                break;
            }
            case none: {
                switch (this.wing_state) {
                    case opened: {
                        this.wing_behavior = Wing_Behavior.close;
                        break block0;
                    }
                    case closed: {
                        this.wing_behavior = Wing_Behavior.open;
                    }
                }
            }
        }
    }

    protected void fireExhaust(Vec3 direction) {
        Vec3 init_pos = this.getNodeBounds(UWBGL_ELevelOfDetail.Low, this.thruster_node, DRAW_HELPER).getCenter();
        if (this.m_Velocity.length() > this.MAX_SPEED) {
            Vec3 nose = this.getMountCenter().plus(direction.normalized());
            float var = (float)Math.sin((double)(System.currentTimeMillis() % 621L) * 0.001 * (double)0.09f) * 1.0f;
            float random = (float)Math.random();
            this.exhaust.add(new ExhaustTrail(this.getPrimitiveBounds(UWBGL_ELevelOfDetail.Low, this.left_wing, DRAW_HELPER).getCenter(), this.m_Velocity.plus(direction.times(this.THRUST_POWER * -0.5f * 2.5E-4f * (0.5f + (float)Math.random())).rotateR(var)).rotateR(2.5f), 0.1f));
            this.exhaust.add(new ExhaustTrail(this.getPrimitiveBounds(UWBGL_ELevelOfDetail.Low, this.right_wing, DRAW_HELPER).getCenter(), this.m_Velocity.plus(direction.times(this.THRUST_POWER * -0.5f * 2.5E-4f * (1.5f - random)).rotateR(-var)).rotateR(-2.5f), 0.1f));
        }
        this.exhaust.add(new ExhaustTrail(init_pos, direction.times(this.THRUST_POWER * -0.5f * ((float)Math.sin(System.currentTimeMillis()) + 1.5707964f) * 0.03f), 0.1f));
        this.exhaust.add(new ExhaustTrail(init_pos, direction.times(this.THRUST_POWER * -0.5f * ((float)Math.sin(System.currentTimeMillis()) + 1.5707964f) * 0.03f), 0.1f));
    }

    protected void closeWings() {
        this.wing_behavior = Wing_Behavior.close;
    }

    protected void openWings() {
        this.wing_behavior = Wing_Behavior.open;
    }

    protected void thrust() {
        this.thrust_behavior = Thrust_Behavior.full;
    }

    protected void brake() {
        this.thrust_behavior = Thrust_Behavior.brake;
    }

    protected void brakeHalt() {
        this.thrust_behavior = Thrust_Behavior.none;
    }

    protected void reverse() {
        this.thrust_behavior = Thrust_Behavior.reverse;
    }

    protected void turnLeft() {
        this.turn_behavior = Turn_Behavior.left;
    }

    protected void turnRight() {
        this.turn_behavior = Turn_Behavior.right;
    }

    protected void turnHalt() {
        this.turn_behavior = Turn_Behavior.none;
    }

    protected void thrustHalt() {
        this.thrust_behavior = Thrust_Behavior.none;
    }

    protected void fireLaser() {
        this.weapon_behavior = Weapon_Behavior.fire_laser;
    }

    protected void fireCannon() {
        this.weapon_behavior = Weapon_Behavior.fire_cannon;
    }

    void fireLaserHalt() {
        this.weapon_behavior = Weapon_Behavior.quit;
    }

    void fireCannonHalt() {
    }

    protected void rotateShip(float delta_t) {
        float theta = this.getXform().getRotationInRadians();
        float delta_theta = this.TURN_POWER * this.m_Omega;
        this.getXform().setRotationRadians(theta -= delta_theta);
        this.m_Direction.x = (float)Math.cos(theta + 1.5707964f);
        this.m_Direction.y = (float)Math.sin(theta + 1.5707964f);
        if ((double)delta_theta > 0.001) {
            this.fireExhaust(Vec3.normFromRadians(-this.m_Direction.angleRadians() + 1.5707964f).timesEquals(delta_theta * 50.0f));
        } else if ((double)delta_theta < 0.001) {
            this.fireExhaust(Vec3.normFromRadians(-this.m_Direction.angleRadians() + 1.5707964f).timesEquals(delta_theta * 50.0f));
        }
    }

    @Override
    public void draw(GL gl, UWBGL_ELevelOfDetail lod, UWBGL_DrawHelper helper) {
        for (ExhaustTrail t : this.exhaust) {
            t.draw(gl, lod, helper);
        }
        super.draw(gl, lod, helper);
    }

    @Override
    public void update(float delta_t) {
        this.rotateShip(delta_t);
        super.update(delta_t);
        ArrayList<ExhaustTrail> toRemove = new ArrayList<ExhaustTrail>(4);
        for (ExhaustTrail t : this.exhaust) {
            t.update(delta_t);
            if (!t.done()) continue;
            toRemove.add(t);
        }
        this.exhaust.removeAll(toRemove);
        switch (this.weapon_behavior) {
            case fire_laser: {
                this.laser.fire(this.getMountCenter(), this.m_Direction, this.m_Velocity);
                this.weapon_behavior = Weapon_Behavior.none;
                this.weapon_state = Weapon_State.firing_laser;
                break;
            }
            case fire_cannon: {
                this.cannon.fire(this.getMountCenter(), this.m_Direction, this.m_Velocity);
                this.weapon_behavior = Weapon_Behavior.none;
            }
            case quit: {
                this.weapon_behavior = Weapon_Behavior.none;
                this.weapon_state = Weapon_State.none;
                this.laser.quitFiring();
                break;
            }
        }
        switch (this.wing_behavior) {
            case none: {
                break;
            }
            case close: {
                this.wing_theta += delta_t * 5.0f;
                if (this.wing_theta >= 0.7853982f) {
                    this.wing_behavior = Wing_Behavior.none;
                    this.wing_state = Wing_State.closed;
                    this.wing_theta = 0.7853982f;
                }
                this.updateWingPos();
                break;
            }
            case open: {
                this.wing_theta -= delta_t * 5.0f;
                if (this.wing_theta <= 0.0f) {
                    this.wing_behavior = Wing_Behavior.none;
                    this.wing_state = Wing_State.opened;
                    this.cur_max_omega = this.BASE_MAX_OMEGA;
                    this.wing_theta = 0.0f;
                }
                this.updateWingPos();
                if (this.wing_behavior != Wing_Behavior.none) break;
                this.thrust_behavior = Thrust_Behavior.none;
            }
        }
        switch (this.wing_state) {
            case closed: {
                this.thrust_behavior = Thrust_Behavior.full;
            }
        }
        switch (this.thrust_behavior) {
            case none: {
                break;
            }
            case full: {
                this.m_Velocity.plusEquals(this.m_Direction.timesEquals(this.THRUST_POWER));
                if (this.wing_state == Wing_State.closed) {
                    this.m_Velocity.plusEquals(this.m_Direction.timesEquals(this.wing_theta / 0.7853982f * this.WING_CLOSED_SPEED));
                    if (this.m_Velocity.length() > this.WING_CLOSED_SPEED) {
                        this.m_Velocity.normalizedEquals().timesEquals(this.WING_CLOSED_SPEED);
                    }
                } else if (this.m_Velocity.length() > this.MAX_SPEED) {
                    this.m_Velocity.normalizedEquals().timesEquals(this.MAX_SPEED);
                }
                this.fireExhaust(this.m_Direction);
                break;
            }
            case brake: {
                if (this.wing_state != Wing_State.opened) break;
                if ((double)this.m_Velocity.length() > (double)this.MAX_SPEED * 0.25) {
                    this.m_Velocity.minusEquals(this.m_Velocity.normalized().timesEquals(this.THRUST_POWER));
                } else {
                    this.m_Velocity.timesEquals(0.9f);
                }
                if ((double)this.m_Omega > 0.01) {
                    this.m_Omega -= this.TURN_POWER;
                    break;
                }
                if ((double)this.m_Omega < -0.01) {
                    this.m_Omega += this.TURN_POWER;
                    break;
                }
                this.m_Omega = 0.0f;
                break;
            }
            case reverse: {
                this.m_Velocity.minusEquals(this.m_Direction.times(this.THRUST_POWER));
                if (this.m_Velocity.length() > this.MAX_SPEED) {
                    this.m_Velocity.normalizedEquals().timesEquals(this.MAX_SPEED);
                }
                this.fireExhaust(this.m_Direction.times(-10.0f));
            }
        }
        switch (this.turn_behavior) {
            case none: {
                this.m_Omega = 0.0f;
                break;
            }
            case left: {
                this.m_Omega -= this.TURN_POWER;
                if (this.m_Omega < -this.cur_max_omega) {
                    this.m_Omega = -this.cur_max_omega;
                }
                this.turn_behavior = Turn_Behavior.none;
                break;
            }
            case right: {
                this.m_Omega += this.TURN_POWER;
                if (this.m_Omega > this.cur_max_omega) {
                    this.m_Omega = this.cur_max_omega;
                }
                this.turn_behavior = Turn_Behavior.none;
            }
        }
        switch (this.weapon_state) {
            case firing_laser: {
                this.laser.continueFiring(this.getMountCenter(), this.m_Direction.normalized());
                break;
            }
            case firing_cannon: {
                this.cannon.continueFiring(this.getMountCenter(), this.m_Direction.normalized());
            }
        }
    }

    protected void updateWingPos() {
        this.left_wing_node.getXform().setRotationRadians(this.wing_theta);
        this.right_wing_node.getXform().setRotationRadians(-this.wing_theta);
        float max_velocity = (this.wing_theta + 0.46f) / 0.7853982f * this.WING_CLOSED_SPEED + this.MAX_SPEED;
        this.m_Velocity.normalizedEquals().timesEquals(this.m_Velocity.length() * 0.8f + max_velocity * 0.15f);
        this.cur_max_omega = (1.0f - this.wing_theta / 0.7853982f) * this.BASE_MAX_OMEGA;
        if (this.cur_max_omega < this.WING_CLOSED_MAX_OMEGA) {
            this.cur_max_omega = this.WING_CLOSED_MAX_OMEGA;
        }
        this.thrust_behavior = Thrust_Behavior.full;
    }

    @Override
    public void setFacingDirection(Vec3 dir) {
        this.getXform().setRotationRadians(dir.angleRadians());
        this.dirty();
    }

    @Override
    public Munition getMunition() {
        if (this.laser.firing()) {
            return this.laser;
        }
        if (this.cannon.firing()) {
            return this.cannon;
        }
        return null;
    }

    public UWBGL_Color getBaseColor() {
        return this.m_BaseColor;
    }

    @Override
    public Vec3 getMountCenter() {
        return this.getNodeBounds(UWBGL_ELevelOfDetail.Low, this.laser_node, DRAW_HELPER).getCenter();
    }

    @Override
    public UWBGL_Color getColor() {
        return this.body_tri.getFlatColor();
    }

    @Override
    public float getRadius() {
        return this.radius;
    }

    private void calcRadius() {
        this.radius = ((UWBGL_BoundingSphere)this.getBounds(UWBGL_ELevelOfDetail.Low, DRAW_HELPER)).getRadius();
    }

    @Override
    public void setTranslation(Vec3 t) {
        this.getXform().setTranslation(t);
    }

    public ArrayList<UWBGL_Primitive> getPrimimatives() {
        ArrayList<UWBGL_Primitive> primitives = new ArrayList<UWBGL_Primitive>();
        primitives.add(this.laser_triangle);
        primitives.add(this.thruster);
        primitives.add(this.left_wing);
        primitives.add(this.right_wing);
        primitives.add(this.body_tri);
        return primitives;
    }

    protected static enum Weapon_Behavior {
        none,
        fire_laser,
        fire_cannon,
        quit;

    }

    protected static enum Weapon_State {
        none,
        firing_laser,
        firing_cannon;

    }

    protected static enum Turn_Behavior {
        none,
        left,
        right;

    }

    protected static enum Thrust_Behavior {
        none,
        full,
        brake,
        reverse;

    }

    protected static enum Wing_State {
        opened,
        closed;

    }

    protected static enum Wing_Behavior {
        none,
        open,
        close;

    }
}

