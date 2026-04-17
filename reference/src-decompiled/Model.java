/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 * 
 * Could not load the following classes:
 *  javax.media.opengl.GL
 *  javax.media.opengl.GLAutoDrawable
 */
import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingSphere;
import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingVolume;
import UWBGL_JavaOpenGL.Primitives.UWBGL_Primitive;
import UWBGL_JavaOpenGL.Primitives.UWBGL_PrimitiveCircle;
import UWBGL_JavaOpenGL.Primitives.UWBGL_PrimitiveRectangle;
import UWBGL_JavaOpenGL.Primitives.UWBGL_PrimitiveTriangle;
import UWBGL_JavaOpenGL.UWBGL_Color;
import UWBGL_JavaOpenGL.UWBGL_DrawHelper;
import UWBGL_JavaOpenGL.UWBGL_EFillMode;
import UWBGL_JavaOpenGL.UWBGL_ELevelOfDetail;
import UWBGL_JavaOpenGL.UWBGL_EShadeMode;
import UWBGL_JavaOpenGL.UWBGL_SceneNode;
import UWBGL_JavaOpenGL.UWBGL_SceneNodeControlListener;
import UWBGL_JavaOpenGL.Util.UWBGL_Util;
import UWBGL_JavaOpenGL.math3d.Vec3;
import java.awt.Rectangle;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.TimerTask;
import javax.media.opengl.GL;
import javax.media.opengl.GLAutoDrawable;

public class Model
implements UWBGL_SceneNodeControlListener,
Common {
    private int m_UniverseRadius;
    private int m_UniverseWidth;
    private int m_UniverseHeight;
    private Rectangle m_UniverseViewBounds;
    private double m_UniverseHypot;
    private UWBGL_BoundingSphere m_UniverseBounds;
    boolean m_UseTextures = true;
    final int m_FPS;
    final float m_DeltaTime;
    private UWBGL_DrawHelper m_draw_helper = DRAW_HELPER;
    private static final int MAX_ASTEROIDS = 100;
    private static final int MAX_ENTITIES = 50;
    private static final int MAX_DEBRIS = 100;
    private static final int MAX_PLANETS = 100;
    private static final int MAX_PARTICLES = 5000;
    private static final float MAX_ASTEROID_SPEED = 200.0f;
    private float m_asteroid_prob_sec;
    private static final float ROTATE_DELTA = 0.1f;
    private static final float PIVOT_DELTA = 1.0f;
    private static final float TRANSLATE_DELTA = 1.0f;
    private static final float SCALE_DELTA = 0.25f;
    private Ship[] m_hero = null;
    private Ship[] m_hero_ship = null;
    private EscapePod[] m_hero_pod = null;
    private Planet m_sun = null;
    private BGStarField m_stars = null;
    private ArrayList<Particle> m_particles = new ArrayList(10000);
    private List<Entity> m_entities = Collections.synchronizedList(new ArrayList(100));
    private List<Debris> m_debris = Collections.synchronizedList(new ArrayList(200));
    private List<Planet> m_planets = Collections.synchronizedList(new ArrayList(200));
    private UWBGL_SceneNode m_CurrentNodeSelection;
    private int m_entity_count = 0;
    private int m_asteroid_count = 0;
    private int m_planet_count = 0;
    private int m_particle_count = 0;
    private boolean RUNNING = true;
    private UWBGL_PrimitiveRectangle[] m_ViewRectangle;
    private UWBGL_BoundingVolume[] m_TestBounds;
    private Player[] m_Players;
    private long m_FrameCount = 0L;
    private boolean m_UseSounds;

    public Model(GameConfig gameConfig) {
        this.m_UniverseRadius = gameConfig.universeRadius;
        this.m_UniverseWidth = 2 * this.m_UniverseRadius;
        this.m_UniverseHeight = 2 * this.m_UniverseRadius;
        this.m_UniverseViewBounds = new Rectangle(0, 0, this.m_UniverseWidth, this.m_UniverseHeight);
        this.m_UniverseHypot = Math.hypot(this.m_UniverseWidth, this.m_UniverseHeight);
        this.m_UniverseBounds = new UWBGL_BoundingSphere(new Vec3(this.m_UniverseRadius, this.m_UniverseRadius), this.m_UniverseRadius);
        this.m_FPS = gameConfig.fps;
        this.m_DeltaTime = 1.0f / (float)this.m_FPS;
        this.m_UseSounds = gameConfig.useSounds;
        this.m_asteroid_prob_sec = gameConfig.astroidProbobility;
        this.m_UseTextures = gameConfig.useTextures;
        this.m_hero = new Ship[2];
        this.m_hero_ship = new Ship[2];
        this.m_hero_pod = new EscapePod[2];
        this.m_hero_ship[0] = new Ship(0, new Vec3(375.0f, 450.0f), 50000.0f, 200.0f, gameConfig.playerHealthPer[0], gameConfig.playerColors[0], gameConfig.useTextures, this.m_DeltaTime);
        this.m_hero_pod[0] = new EscapePod(0, new Vec3(375.0f, 450.0f), 50000.0f, 10.0f, gameConfig.playerColors[0], gameConfig.useTextures, this.m_DeltaTime);
        this.m_hero[0] = this.m_hero_ship[0];
        this.m_hero_ship[1] = new Ship(1, new Vec3(375.0f, 500.0f), 50000.0f, 200.0f, gameConfig.playerHealthPer[1], gameConfig.playerColors[1], gameConfig.useTextures, this.m_DeltaTime);
        this.m_hero_pod[1] = new EscapePod(1, new Vec3(375.0f, 450.0f), 50000.0f, 10.0f, gameConfig.playerColors[1], gameConfig.useTextures, this.m_DeltaTime);
        this.m_hero[1] = this.m_hero_ship[1];
        this.m_Players = new Player[2];
        this.m_Players[0] = new Player(0, gameConfig.playerNames[0], this.m_hero[0], gameConfig.playerColors[0]);
        this.m_Players[1] = new Player(1, gameConfig.playerNames[1], this.m_hero[1], gameConfig.playerColors[1]);
        if (gameConfig.usePlanets) {
            float spacing;
            float planetRadius;
            float sunRadius = 200.0f;
            Vec3 sunCenter = new Vec3(this.m_UniverseRadius, this.m_UniverseRadius, 0.7f);
            this.m_sun = Planet.makeSun(sunCenter, sunRadius, "sun_3.jpg", gameConfig.useTextures);
            for (float planetMinOrbit = sunRadius + 20.0f; planetMinOrbit < (float)this.m_UniverseRadius && this.m_planets.size() < 99; planetMinOrbit += planetRadius * 2.0f + spacing) {
                planetRadius = UWBGL_Util.randomNumber(15.0f, 150.0f);
                spacing = UWBGL_Util.randomNumber(10.0f, 50.0f);
                float startAngle = UWBGL_Util.randomNumber(0.0f, (float)Math.PI * 2);
                float orbitDistance = planetMinOrbit + planetRadius + spacing;
                if (orbitDistance + planetRadius >= (float)this.m_UniverseRadius) break;
                float maxSpeed = (float)Math.PI * 2 / orbitDistance * 14.0f;
                float omega = UWBGL_Util.randomNumber(-maxSpeed, maxSpeed);
                Vec3 planetOrbit = Vec3.normFromRadians(startAngle).timesEquals(orbitDistance);
                Planet m_planet = Planet.makePlanet(planetOrbit, planetRadius, omega, Planet.getRandomImage(), this.m_sun, gameConfig.useTextures);
                this.addPlanet(m_planet);
                this.m_sun.insertChildNode(m_planet);
            }
            this.addPlanet(this.m_sun);
        }
        this.m_draw_helper.setShadeMode(UWBGL_EShadeMode.Flat);
        this.m_ViewRectangle = new UWBGL_PrimitiveRectangle[2];
        this.m_ViewRectangle[0] = new UWBGL_PrimitiveRectangle(new Vec3(10.0f, 10.0f, 0.0f), new Vec3(50.0f, 50.0f, 0.0f));
        this.m_ViewRectangle[1] = new UWBGL_PrimitiveRectangle(new Vec3(10.0f, 10.0f, 0.0f), new Vec3(100.0f, 100.0f, 0.0f));
        this.m_ViewRectangle[0].setFlatColor(UWBGL_Color.WHITE);
        this.m_ViewRectangle[1].setFlatColor(UWBGL_Color.WHITE);
        this.m_ViewRectangle[0].setFillMode(UWBGL_EFillMode.Outline);
        this.m_ViewRectangle[1].setFillMode(UWBGL_EFillMode.Outline);
        this.m_CurrentNodeSelection = this.m_sun;
        this.m_TestBounds = new UWBGL_BoundingVolume[4];
        if (gameConfig.useStarfield) {
            this.m_stars = new BGStarField(this.m_UniverseBounds.getCenter(), this.m_UniverseBounds.getRadius(), 0.0025f);
        }
        if (this.m_UseSounds) {
            new PlayWave("./rec/sounds/complete.wav").start();
        }
    }

    int gameOver() {
        int winner = -1;
        int deadPlayer = -1;
        for (int i = 0; i < 2; ++i) {
            if (!this.m_hero_ship[i].dead()) {
                winner = i;
                continue;
            }
            if (!this.m_hero_ship[i].dead() || this.m_Players[i].getPlanetCount() > 0) continue;
            deadPlayer = i;
        }
        if (deadPlayer >= 0) {
            return winner;
        }
        return -1;
    }

    void setStarFieldVisible(boolean b) {
        if (this.m_stars != null) {
            this.m_stars.setVisible(b);
        }
    }

    private float applyDamage(HasLife hasLife, CausesDamage causesDamage) {
        float damage = causesDamage.damageAmount(hasLife.getVelocity());
        hasLife.translateLife(-damage);
        return damage;
    }

    private void destroyPrimatives(ArrayList<UWBGL_Primitive> primatives, Vec3 location, Vec3 velocity, float variation) {
        float delta_theta = (float)Math.PI * 2 / (float)primatives.size();
        float theta = (float)Math.random() * 2.0f * (float)Math.PI;
        for (UWBGL_Primitive primate : primatives) {
            UWBGL_Primitive flack = primate.clone();
            Vec3 dir = new Vec3(50.0f).rotateREquals(theta);
            dir.plusEquals(velocity);
            flack.moveTo(location);
            if (this.m_UseTextures && primate.getTexturingEnabled()) {
                flack.setTextureFileName(primate.getTextureFileName());
                flack.setTexturing(true);
            }
            ++this.m_particle_count;
            Debris debri = new Debris(new Vec3(), flack, dir, this.m_UseTextures);
            debri.setOmega(variation);
            this.m_debris.add(debri);
            theta += delta_theta;
        }
    }

    public void doPhysics() {
        if (!this.RUNNING) {
            return;
        }
        ArrayList<Entity> entities_to_remove = new ArrayList<Entity>();
        ArrayList<Particle> particles_to_remove = new ArrayList<Particle>();
        ArrayList<Debris> debris_to_remove = new ArrayList<Debris>();
        boolean do_particle_grav = false;
        boolean do_asteroid_grav = false;
        if (this.m_FrameCount % 3L == 0L) {
            do_particle_grav = true;
        }
        if (this.m_FrameCount % 7L == 0L) {
            do_asteroid_grav = true;
        }
        for (Planet planet : this.m_planets) {
            int prevOwnerId = planet.getOwnerId();
            planet.update(this.m_DeltaTime);
            if (planet.getOwnerId() == prevOwnerId) continue;
            if (this.m_UseSounds) {
                new PlayWave("./rec/sounds/hourglass.wav").start();
            }
            if (prevOwnerId >= 0) {
                this.m_Players[prevOwnerId].losePlanet(planet);
            }
            if (planet.getOwnerId() < 0) continue;
            this.m_Players[planet.getOwnerId()].claimPlanet(planet);
        }
        for (int i = 0; i < 2; ++i) {
            float damage;
            Vec3 intersection;
            Vec3 collision_dir;
            ArrayList<Debris> arrayList = new ArrayList<Debris>(this.m_debris);
            this.changePodToShip(i);
            this.m_hero[i].update(this.m_DeltaTime);
            Vec3 fudge = this.m_hero[i].getBounds(UWBGL_ELevelOfDetail.Low, DRAW_HELPER).isContainedBy(this.m_UniverseBounds.getCenter(), this.m_UniverseBounds.getRadius());
            this.m_hero[i].getXform().translateTranslation(fudge);
            Munition boom = this.m_hero[i].getMunition();
            if (boom != null) {
                Vec3 intercept;
                Debris to_add_debris = boom.update(this.m_DeltaTime);
                if (to_add_debris != null) {
                    this.m_debris.add(to_add_debris);
                }
                for (Debris a : this.m_debris) {
                    if (!boom.intersects(null, a, this.m_draw_helper)) continue;
                    intercept = boom.findIntersect(null, a, this.m_draw_helper);
                    boom.collidedAt(intercept);
                    this.effectLaserVsEntity(boom.getFacingDirection(), a.getLocation(), intercept, a.getColor(), boom.damageAmount(), 10.0f);
                    this.applyDamage(a, (CausesDamage)((Object)boom));
                    if (!(a.getLife() <= 0.0f)) continue;
                    entities_to_remove.add(a);
                }
                for (Planet p : this.m_planets) {
                    if (!boom.intersects(this.m_sun, p, this.m_draw_helper)) continue;
                    if (this.m_UseSounds) {
                        new PlayWave("./rec/sounds/laser.wav").start();
                    }
                    intercept = boom.findIntersect(this.m_sun, p, this.m_draw_helper);
                    boom.collidedAt(intercept);
                    this.effectLaserVsEntity(boom.getFacingDirection(), p.getLocation(), intercept, p.getColor(), boom.damageAmount(), 1.0f);
                }
                for (int j = 0; j < 2; ++j) {
                    if (j == i || !boom.intersects(null, this.m_hero[j], this.m_draw_helper)) continue;
                    Vec3 intercept2 = boom.findIntersect(null, this.m_hero[j], this.m_draw_helper);
                    boom.collidedAt(intercept2);
                    this.m_hero[j].translateLife(-boom.damageAmount());
                    this.effectLaserVsEntity(boom.getFacingDirection(), this.m_hero[j].getLocation(), intercept2, this.m_hero[j].getColor(), boom.damageAmount(), 10.0f);
                    if (!this.m_hero[j].dead()) continue;
                    if (this.m_UseSounds) {
                        new PlayWave("./rec/sounds/explode.wav").start();
                    }
                    this.destroyPrimatives(this.m_hero[j].getPrimimatives(), this.m_hero[j].getLocation(), this.m_hero[j].getVelocity(), 1.0f);
                    this.changeShipToPod(j, boom.getFacingDirection().times(boom.damageAmount()));
                }
            }
            for (Debris d : arrayList) {
                if (!this.m_hero[i].intersects(null, d, this.m_draw_helper)) continue;
                this.collideEntities(this.m_hero[i], d);
                collision_dir = this.m_hero[i].getLocation().minus(d.getLocation()).normalizedEquals();
                intersection = this.m_hero[i].getLocation().minus(collision_dir.times(this.m_hero[i].getRadius()));
                damage = this.applyDamage(this.m_hero[i], d);
                d.translateLife(-damage);
                this.effectLaserVsEntity(d.getFacingDirection().times(-1.0f), this.m_hero[i].getLocation(), intersection, this.m_hero[i].getColor(), damage, 10.0f);
                if (!this.m_hero[i].dead()) continue;
                if (this.m_UseSounds) {
                    new PlayWave("./rec/sounds/explode.wav").start();
                }
                this.destroyPrimatives(this.m_hero[i].getPrimimatives(), this.m_hero[i].getLocation(), this.m_hero[i].getVelocity(), 1.0f);
                this.changeShipToPod(i, d.getFacingDirection().times(d.damageAmount(this.m_hero[i].getVelocity())));
            }
            for (int j = 0; j < 2; ++j) {
                if (j == i || !this.m_hero[i].intersects(null, this.m_hero[j], this.m_draw_helper)) continue;
                if (this.m_UseSounds) {
                    new PlayWave("./rec/sounds/slideoops.wav").start();
                }
                this.collideEntities(this.m_hero[i], this.m_hero[j]);
            }
            for (Planet p : this.m_planets) {
                if (this.m_hero[i].intersects(this.m_sun, p, this.m_draw_helper)) {
                    if (!this.collideWithPlanet(this.m_hero[i], p, this.m_sun, this.m_draw_helper)) break;
                    if (this.m_UseSounds) {
                        new PlayWave("./rec/sounds/slideoops.wav").start();
                    }
                    collision_dir = p.getLocation().minus(this.m_hero[i].getLocation()).normalizedEquals();
                    intersection = p.getLocation().minus(collision_dir.times(p.getRadius()));
                    damage = this.applyDamage(this.m_hero[i], p);
                    this.m_hero[i].translateLife(-damage);
                    this.effectLaserVsEntity(collision_dir, this.m_hero[i].getLocation(), intersection, this.m_hero[i].getColor(), damage, 5.0f);
                    if (!this.m_hero[i].dead()) break;
                    if (this.m_UseSounds) {
                        new PlayWave("./rec/sounds/explode.wav").start();
                    }
                    this.destroyPrimatives(this.m_hero[i].getPrimimatives(), this.m_hero[i].getLocation(), this.m_hero[i].getVelocity(), 1.0f);
                    this.changeShipToPod(i, this.m_hero[i].getFacingDirection().times(-damage));
                    break;
                }
                this.applyGravity(this.m_hero[i], p, this.m_draw_helper, 1);
            }
            Vec3 loc = this.m_hero[i].getXform().getTranslation();
            this.m_ViewRectangle[i].moveTo(loc);
        }
        ArrayList<Debris> working_debris = new ArrayList<Debris>(this.m_debris);
        while (!working_debris.isEmpty()) {
            Vec3 collision_dir;
            Debris debris = working_debris.get(0);
            for (Planet p : this.m_planets) {
                if (do_asteroid_grav) {
                    this.applyGravity(debris, p, this.m_draw_helper, 7);
                }
                if (!debris.getBounds(UWBGL_ELevelOfDetail.Low, DRAW_HELPER).intersects(p.getTranslatedBounds(UWBGL_ELevelOfDetail.Low)) || !this.collideWithPlanet(debris, p, this.m_sun, this.m_draw_helper)) continue;
                debris.translateLife(-debris.getVelocity().length() * 0.05f);
                collision_dir = p.getLocation().minus(debris.getLocation()).normalizedEquals();
                Vec3 intersection = p.getLocation().minus(collision_dir.times(p.getRadius()));
                float damage = this.applyDamage(debris, p);
                this.effectLaserVsEntity(collision_dir, debris.getLocation(), intersection, debris.getColor(), damage, 5.0f);
                break;
            }
            for (Debris b : working_debris) {
                if (debris == b || !debris.intersects(null, b, this.m_draw_helper)) continue;
                collision_dir = debris.getLocation().minus(b.getLocation()).normalizedEquals();
                Vec3 intersection = debris.getLocation().minus(collision_dir.times(debris.getRadius()));
                float damage = this.applyDamage(debris, b);
                this.applyDamage(b, debris);
                this.effectLaserVsEntity(collision_dir, b.getLocation(), intersection, b.getColor(), damage, 5.0f);
                this.effectLaserVsEntity(collision_dir.timesEquals(-1.0f), debris.getLocation(), intersection, debris.getColor(), damage, 5.0f);
                this.collideEntities(debris, b);
            }
            debris.update(this.m_DeltaTime);
            UWBGL_BoundingVolume bound = debris.getBounds(UWBGL_ELevelOfDetail.Low, this.m_draw_helper);
            if (!bound.intersects(this.m_UniverseBounds)) {
                entities_to_remove.add(debris);
                --this.m_asteroid_count;
            }
            if (debris.dead()) {
                --this.m_asteroid_count;
                debris_to_remove.add(debris);
            }
            working_debris.remove(debris);
        }
        this.m_debris.removeAll(debris_to_remove);
        for (Particle p : this.m_particles) {
            for (Planet planet : this.m_planets) {
                if (do_particle_grav) {
                    this.applyGravity(p, planet, this.m_draw_helper, 3);
                }
                if (!p.getBounds().intersects(planet.getTranslatedBounds(UWBGL_ELevelOfDetail.Low))) continue;
                this.collideWithPlanet(p, planet, this.m_sun, this.m_draw_helper);
                break;
            }
            p.update(this.m_DeltaTime);
            if (!p.done()) continue;
            particles_to_remove.add(p);
        }
        this.m_particle_count -= particles_to_remove.size();
        this.m_particles.removeAll(particles_to_remove);
        this.removeAll(entities_to_remove);
        this.spawnRandomAsteroid();
        ++this.m_FrameCount;
    }

    private void changeShipToPod(int i, Vec3 force_dir) {
        this.m_hero[i] = this.m_hero_pod[i];
        this.m_hero_pod[i].setChangeToShip(false);
        this.m_hero_pod[i].setTranslation(this.m_hero_ship[i].getLocation());
        this.m_hero_pod[i].setVelocity(this.m_hero_ship[i].getVelocity().plus(force_dir));
        this.m_hero_pod[i].setFacingDirection(this.m_hero_ship[i].getFacingDirection());
        this.m_hero_pod[i].setLife(0.0f);
        this.getPlayer(i).setShip(this.m_hero[i]);
    }

    private void changePodToShip(int i) {
        if (this.m_hero[i].getClass() == EscapePod.class && ((EscapePod)this.m_hero[i]).changeToShip()) {
            this.m_hero[i] = this.m_hero_ship[i];
            this.m_hero[i].setDead(false);
            this.m_hero[i].setTranslation(this.m_hero_pod[i].getLocation());
            this.m_hero[i].setVelocity(this.m_hero_pod[i].getVelocity());
            this.m_hero[i].setFacingDirection(this.m_hero_pod[i].getFacingDirection());
            this.m_hero[i].setLife(this.m_hero[i].getLifeMax());
            this.getPlayer(i).setShip(this.m_hero[i]);
        }
    }

    private void applyGravity(ObeysForces e, Planet p, UWBGL_DrawHelper helper, int scale) {
        float m1 = e.getMass();
        float m2 = p.getMass();
        Vec3 e_loc = e.getLocation();
        Vec3 p_loc = p.getBounds(UWBGL_ELevelOfDetail.Low, helper).getCenter();
        Vec3 angle = p_loc.minus(e_loc).normalizedEquals();
        float distance = e_loc.distanceTo(p_loc);
        float force = 0.005f * m1 * m2 / (distance * distance);
        e.applyForce(force * (float)scale, angle);
    }

    private void collideEntities(Entity a, Entity b) {
        float j_velocity_percent;
        float i_velocity_percent;
        Vec3 angle = a.getLocation().minus(b.getLocation()).normalizedEquals();
        Vec3 v1 = a.getVelocity();
        Vec3 v2 = b.getVelocity();
        float m1 = a.getMass();
        float m2 = b.getMass();
        float total_velocity = v1.length() + v2.length();
        if (total_velocity < 1.0E-7f) {
            i_velocity_percent = 0.5f;
            j_velocity_percent = 0.5f;
        } else {
            i_velocity_percent = v1.length() / total_velocity;
            j_velocity_percent = v2.length() / total_velocity;
        }
        a.setVelocity(v1.clone().timesEquals(m1 - m2).plusEquals(v2.clone().timesEquals(2.0f * m2)).divideEquals(m1 + m2).timesEquals(0.9f));
        b.setVelocity(v2.clone().timesEquals(m2 - m1).plusEquals(v1.clone().timesEquals(2.0f * m1)).divideEquals(m1 + m2).timesEquals(0.9f));
        UWBGL_BoundingSphere a_bounds = (UWBGL_BoundingSphere)a.getBounds(UWBGL_ELevelOfDetail.Low, this.m_draw_helper);
        UWBGL_BoundingSphere b_bounds = (UWBGL_BoundingSphere)b.getBounds(UWBGL_ELevelOfDetail.Low, this.m_draw_helper);
        float overlap = a_bounds.getCenter().minus(b_bounds.getCenter()).length() - a_bounds.getRadius() - b_bounds.getRadius();
        a.getXform().translateTranslation(angle.times(-i_velocity_percent * overlap * 1.0f));
        a.dirty();
        b.getXform().translateTranslation(angle.times(j_velocity_percent * overlap * 1.0f));
        b.dirty();
    }

    private boolean collideWithPlanet(ObeysForces e, Planet p, UWBGL_SceneNode root, UWBGL_DrawHelper helper) {
        UWBGL_PrimitiveCircle planet_circle = p.getPlanetPrimitive();
        UWBGL_BoundingSphere planet_bounds = (UWBGL_BoundingSphere)p.getTranslatedBounds(UWBGL_ELevelOfDetail.Low);
        Vec3 planet_center = p.getLocation();
        UWBGL_BoundingSphere sp_bounds = (UWBGL_BoundingSphere)root.getPrimitiveBounds(UWBGL_ELevelOfDetail.High, p.getsSpacePort(), helper);
        if (p.hasSpacePort() && sp_bounds.intersects(e.getBounds(UWBGL_ELevelOfDetail.High, helper))) {
            if (!planet_bounds.intersects(e.getBounds(UWBGL_ELevelOfDetail.High, helper))) {
                return false;
            }
            if (e.getClass() == Ship.class || e.getClass() == EscapePod.class) {
                Ship ship = (Ship)e;
                p.setLandingShip(ship);
                Vec3 offset = sp_bounds.getCenter().minus(ship.getLocation());
                float distance = offset.length() * 3.0f;
                ship.getVelocity().timesEquals(0.94f);
                ship.applyForce(distance, offset);
                return false;
            }
        }
        Vec3 angle = e.getLocation().minus(planet_center).normalized();
        float distance = e.getRadius() + planet_circle.getRadius();
        e.setTranslation(planet_center.plus(angle.times(distance)));
        float in_theta = (float)Math.atan2(e.getVelocity().y, e.getVelocity().x);
        float norm_theta = (float)Math.atan2(-angle.y, -angle.x);
        float out_theta = (float)Math.PI + in_theta - (in_theta - norm_theta) * 2.0f;
        e.setVelocity(Vec3.normFromRadians(out_theta).timesEquals(e.getVelocity().length() * 0.5f));
        return true;
    }

    public UWBGL_SceneNode getRootNode() {
        return this.m_sun;
    }

    public Player getPlayer(int player) {
        return this.m_Players[player];
    }

    public Ship getHero(int player) {
        return this.m_hero[player];
    }

    public UWBGL_SceneNode getCurrentNode() {
        return this.m_CurrentNodeSelection;
    }

    public void setCurrentNode(UWBGL_SceneNode node) {
        this.m_CurrentNodeSelection = node;
    }

    void display(GLAutoDrawable drawable) {
        GL gl = drawable.getGL();
        if (this.m_stars != null) {
            this.m_stars.draw(gl, UWBGL_ELevelOfDetail.High, this.m_draw_helper);
        }
        if (this.m_sun != null) {
            this.m_sun.draw(gl, UWBGL_ELevelOfDetail.High, this.m_draw_helper);
        }
        for (int i = 0; i < 2; ++i) {
            this.m_hero[i].draw(gl, UWBGL_ELevelOfDetail.High, this.m_draw_helper);
            Munition boom = this.m_hero[i].getMunition();
            if (boom == null) continue;
            boom.draw(gl, UWBGL_ELevelOfDetail.High, this.m_draw_helper);
        }
        for (Debris a : this.m_debris) {
            a.draw(gl, UWBGL_ELevelOfDetail.High, this.m_draw_helper);
        }
        for (int i = 0; i < 2; ++i) {
            this.m_ViewRectangle[i].draw(gl, this.m_draw_helper.getLOD(), this.m_draw_helper);
        }
        for (Particle p : this.m_particles) {
            p.draw(gl, UWBGL_ELevelOfDetail.High, this.m_draw_helper);
        }
        if (!this.RUNNING) {
            // empty if block
        }
        for (int i = 0; i < this.m_TestBounds.length; ++i) {
            if (this.m_TestBounds[i] == null) continue;
            this.m_TestBounds[i].draw(gl, this.m_draw_helper);
        }
    }

    public void cacheData() {
    }

    public UWBGL_PrimitiveRectangle getViewBounds(int player) {
        return this.m_ViewRectangle[player];
    }

    public void setViewBounds(int player, Vec3 p1, Vec3 p2) {
        this.m_ViewRectangle[player].setMin(p1);
        this.m_ViewRectangle[player].setMax(p2);
    }

    public void setViewBoundsVisible(int player, boolean t) {
        this.m_ViewRectangle[player].setVisible(t);
    }

    @Override
    public void rotateNode(UWBGL_SceneNode node, int dir) {
        float new_theta = node.getXform().getRotationInRadians() + 0.1f * (float)dir;
        node.getXform().setRotationRadians(new_theta);
    }

    @Override
    public void setRotationNode(UWBGL_SceneNode node, float theta) {
        node.getXform().setRotationRadians(theta);
    }

    @Override
    public void pivotNodeX(UWBGL_SceneNode node, int dir) {
        Vec3 new_pivot = node.getXform().getPivot().plus(new Vec3((float)dir * 1.0f));
        node.getXform().setPivot(new_pivot);
    }

    @Override
    public void pivotNodeY(UWBGL_SceneNode node, int dir) {
        Vec3 new_pivot = node.getXform().getPivot().plus(new Vec3(0.0f, (float)dir * 1.0f));
        node.getXform().setPivot(new_pivot);
    }

    @Override
    public void translateNodeX(UWBGL_SceneNode node, int dir) {
        Vec3 new_loc = node.getXform().getTranslation().plus(new Vec3((float)dir * 1.0f));
        node.getXform().setTranslation(new_loc);
    }

    @Override
    public void translateNodeY(UWBGL_SceneNode node, int dir) {
        Vec3 new_loc = node.getXform().getTranslation().plus(new Vec3(0.0f, (float)dir * 1.0f));
        node.getXform().setTranslation(new_loc);
    }

    @Override
    public void scaleNodeX(UWBGL_SceneNode node, int dir) {
        Vec3 new_scale = node.getXform().getScale().plus(new Vec3((float)dir * 0.25f));
        node.getXform().setScale(new_scale);
    }

    @Override
    public void scaleNodeY(UWBGL_SceneNode node, int dir) {
        Vec3 new_scale = node.getXform().getScale().plus(new Vec3(0.0f, (float)dir * 0.25f));
        node.getXform().setScale(new_scale);
    }

    @Override
    public void setShowPivot(UWBGL_SceneNode node, boolean show) {
        node.setPivotVisible(show);
    }

    private void addPlanet(Planet p) {
        if (this.m_planet_count < 100) {
            this.m_planets.add(p);
            ++this.m_planet_count;
        }
    }

    public int getPlanetCount() {
        return this.m_planets.size() - 1;
    }

    public void setPause(boolean b) {
        this.RUNNING = !b;
    }

    public boolean getPause() {
        return !this.RUNNING;
    }

    private void spawnRandomAsteroid() {
        if (this.m_asteroid_count > 100) {
            return;
        }
        if ((double)(this.m_asteroid_prob_sec * this.m_DeltaTime) > Math.random()) {
            Vec3 asteroid_location = Vec3.normFromRadians((float)(Math.random() * 2.0 * Math.PI)).timesEquals((float)this.m_UniverseRadius * 0.99f).plusEquals(this.m_UniverseBounds.getCenter());
            float size = 5.0f * (float)Math.random() + 2.5f;
            if (Math.random() > (double)0.98f) {
                size = (float)((double)size * (10.0 * Math.random()));
            }
            UWBGL_PrimitiveCircle toid = new UWBGL_PrimitiveCircle(new Vec3(), size);
            float maxAngle = 1.5707964f;
            Vec3 aim = this.m_UniverseBounds.getCenter().minus(asteroid_location).normalizedEquals().rotateREquals(UWBGL_Util.randomNumber(-maxAngle, maxAngle));
            Vec3 v = aim.timesEquals(200.0f * (float)Math.random());
            Debris new_asteroid = new Debris(asteroid_location, toid, v, 0.01f, this.m_UseTextures);
            new_asteroid.setOmega((float)Math.random() * 10.0f);
            ++this.m_asteroid_count;
            this.m_debris.add(new_asteroid);
        }
    }

    private void removeAll(ArrayList<Entity> to_remove) {
        for (Entity e : to_remove) {
            if (e.getClass() == Debris.class || e.getClass() == Shell.class) {
                this.m_debris.remove(e);
                continue;
            }
            System.err.println("$ ERROR!!! Removing unsupported class: " + e.getClass());
            System.exit(-1);
        }
    }

    private void effectLaserVsEntity(Vec3 flack_dir, Vec3 entity_center, Vec3 intercept, UWBGL_Color color, float damage, float scale) {
        if ((float)this.m_particle_count + damage * scale > 5000.0f) {
            return;
        }
        Vec3 e_center = entity_center;
        Vec3 normal = intercept.minus(e_center);
        float laser_theta = (float)Math.atan2(flack_dir.y, flack_dir.x);
        float normal_theta = (float)Math.atan2(normal.y, normal.x);
        float flack_theta = (float)Math.PI + laser_theta - (laser_theta - normal_theta) * 2.0f;
        float p1_x = intercept.x + (float)Math.cos(flack_theta);
        float p1_y = intercept.y + (float)Math.sin(flack_theta);
        float p2_x = intercept.x + (float)Math.cos(flack_theta + 1.0f);
        float p2_y = intercept.y + (float)Math.sin(flack_theta + 1.0f);
        float p3_x = intercept.x + (float)Math.cos(flack_theta - 1.0f);
        float p3_y = intercept.y + (float)Math.sin(flack_theta - 1.0f);
        float flack_count = damage * scale;
        int i = 0;
        while ((float)i < flack_count * 1.0f) {
            float rand_angle = (float)Math.random() * (float)Math.random() * 1.5f;
            Vec3 flack_vector = Vec3.normFromRadians(flack_theta + rand_angle).timesEquals(damage * (float)Math.random() * 20.0f);
            UWBGL_PrimitiveTriangle flack_particle = new UWBGL_PrimitiveTriangle(intercept.clone().set(p1_x, p1_y, -0.9f), intercept.clone().set(p2_x, p2_y, -0.9f), intercept.clone().set(p3_x, p3_y, -0.9f));
            ++this.m_particle_count;
            this.m_particles.add(new Particle(flack_vector, color, flack_particle, 0.5882353f));
            ++i;
        }
    }

    public float getDeltaTime() {
        return this.m_DeltaTime;
    }

    public float getFPS() {
        return this.m_FPS;
    }

    public Rectangle getWorldViewBounds() {
        return this.m_UniverseViewBounds;
    }

    public double getWorldViewHypot() {
        return this.m_UniverseHypot;
    }

    class BeginUpdate
    extends TimerTask {
        BeginUpdate() {
        }

        @Override
        public void run() {
            Model.this.doPhysics();
        }
    }
}

