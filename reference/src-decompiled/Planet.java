/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 */
import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingList;
import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingSphere;
import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingVolume;
import UWBGL_JavaOpenGL.Primitives.UWBGL_Primitive;
import UWBGL_JavaOpenGL.Primitives.UWBGL_PrimitiveCircle;
import UWBGL_JavaOpenGL.Primitives.UWBGL_PrimitiveLine;
import UWBGL_JavaOpenGL.Primitives.UWBGL_PrimitiveList;
import UWBGL_JavaOpenGL.Primitives.UWBGL_PrimitivePolygon;
import UWBGL_JavaOpenGL.UWBGL_Color;
import UWBGL_JavaOpenGL.UWBGL_DrawHelper;
import UWBGL_JavaOpenGL.UWBGL_EFillMode;
import UWBGL_JavaOpenGL.UWBGL_ELevelOfDetail;
import UWBGL_JavaOpenGL.UWBGL_EShadeMode;
import UWBGL_JavaOpenGL.UWBGL_SceneNode;
import UWBGL_JavaOpenGL.Util.UWBGL_Util;
import UWBGL_JavaOpenGL.math3d.Vec3;
import java.io.File;

public class Planet
extends AbstractEntity
implements Common,
CausesDamage {
    public static final float MIN_RADIUS = 15.0f;
    public static final float MAX_RADIUS = 150.0f;
    public static final int CAPTURE_TIME = 4;
    public static final int BUILD_SHIP_TIME = 8;
    public static final float HEALTH_GAIN_PER_SEC = 3.0f;
    public static final String IMAGE_RESOURCE_DIR = "planets/";
    private Planet m_Parent = null;
    private boolean m_Orbits_Parent = false;
    private Vec3 m_location_cache = new Vec3();
    private boolean m_location_dirty = true;
    private boolean m_translated_bounds_high_dirty = true;
    private boolean m_translated_bounds_low_dirty = true;
    private UWBGL_BoundingVolume m_Translated_High_Bounds = new UWBGL_BoundingList();
    private UWBGL_BoundingVolume m_Translated_Low_Bounds = new UWBGL_BoundingSphere();
    UWBGL_PrimitiveCircle planet_circle;
    UWBGL_Primitive m_Flag;
    UWBGL_Primitive space_port;
    WrapperEntity planetWrapper;
    UWBGL_SceneNode space_port_node = new UWBGL_SceneNode("Space_Port");
    int m_OwnedBy = -1;
    Ship m_LandingShip = null;
    Ship m_LandingShipPrevRound = null;
    float m_TakingOwnershipTime = 0.0f;
    float m_BuildingNewShipTime = 0.0f;

    private Planet(String name, Vec3 pos, float radius) {
        super(name, pos);
        this.planet_circle = new UWBGL_PrimitiveCircle(new Vec3(0.0f, 0.0f, 0.7f), radius);
        this.planet_circle.setFillMode(UWBGL_EFillMode.Solid);
        this.planet_circle.setFlatColor(UWBGL_Color.random());
        this.m_Mass = this.planet_circle.getArea() * 750.0f;
    }

    public static Planet makePlanet(Vec3 pos, float radius, float orbitRate, String texture, Planet parent, boolean useTextures) {
        Planet newPlanet = new Planet("Planet", pos, radius);
        if (parent != null) {
            newPlanet.m_Orbits_Parent = true;
            newPlanet.m_Parent = parent;
        }
        newPlanet.m_Omega = orbitRate;
        newPlanet.planetWrapper = new WrapperEntity();
        newPlanet.planetWrapper.m_HasParent = true;
        newPlanet.planetWrapper.m_Omega = UWBGL_Util.randomNumber(-0.5235988f, 0.5235988f);
        newPlanet.insertChildNode(newPlanet.planetWrapper);
        newPlanet.planetWrapper.insertChildNode(newPlanet.space_port_node);
        newPlanet.planet_circle.setDrawBoundingVolume(true);
        newPlanet.space_port = Planet.makeSpacePort(radius);
        newPlanet.space_port_node.setPrimitive(newPlanet.space_port);
        newPlanet.m_Flag = Planet.makeFlag(radius);
        UWBGL_SceneNode flagNode = new UWBGL_SceneNode("Flag");
        flagNode.setPrimitive(newPlanet.m_Flag);
        newPlanet.planetWrapper.insertChildNode(flagNode);
        if (useTextures && texture != null) {
            newPlanet.setTextureFileName(texture);
            newPlanet.planet_circle.setTexturing(true);
            newPlanet.space_port.setTextureFileName("./rec/landing.png");
            newPlanet.space_port.setTexturing(true);
        }
        newPlanet.planetWrapper.setPrimitive(newPlanet.planet_circle);
        newPlanet.m_HasParent = true;
        newPlanet.getXform().setPivot(pos.times(-1.0f));
        newPlanet.calcLocation();
        return newPlanet;
    }

    private static UWBGL_Primitive makeSpacePort(float planetRadius) {
        int i;
        float depthFactor = 0.4f;
        float depth = planetRadius * depthFactor;
        int numOuterPoints = Math.max(15, 1);
        int numInnerPoints = 7;
        Vec3[] outline = new Vec3[numOuterPoints + numInnerPoints];
        float angle = 94.24778f / planetRadius;
        for (i = 0; i < numOuterPoints; ++i) {
            outline[i] = new Vec3();
            outline[i].x = (float)Math.cos((float)i * angle / (float)numOuterPoints) * planetRadius;
            outline[i].y = (float)Math.sin((float)i * angle / (float)numOuterPoints) * planetRadius;
            outline[i].z = 0.59999996f;
        }
        if (angle < 2.7488937f) {
            for (i = 0; i < numInnerPoints; ++i) {
                outline[i + numOuterPoints] = new Vec3();
                outline[i + numOuterPoints].x = (float)Math.cos((float)(numInnerPoints - i - 1) * angle / (float)numInnerPoints) * depth;
                outline[i + numOuterPoints].y = (float)Math.sin((float)(numInnerPoints - i - 1) * angle / (float)numInnerPoints) * depth;
                outline[i + numOuterPoints].z = 0.59999996f;
            }
        } else {
            for (i = 0; i < numInnerPoints; ++i) {
                outline[i + numOuterPoints] = outline[0].minus(outline[numOuterPoints - 1]).divideEquals(numInnerPoints).timesEquals(i + 1).plusEquals(outline[numOuterPoints - 1]);
                outline[i + numOuterPoints].z = 0.59999996f;
            }
        }
        UWBGL_PrimitivePolygon spacePort = new UWBGL_PrimitivePolygon(outline);
        spacePort.setFillMode(UWBGL_EFillMode.Solid);
        spacePort.setFlatColor(UWBGL_Color.WHITE);
        return spacePort;
    }

    private static UWBGL_Primitive makeFlag(float planetRadius) {
        float scale = planetRadius * 0.6f;
        UWBGL_PrimitiveList flag = new UWBGL_PrimitiveList();
        UWBGL_Primitive temp = new UWBGL_PrimitiveCircle(new Vec3(), 6.0f);
        flag.add(temp);
        float flagHeight = 0.3f * scale;
        float flagWidth = 0.45f * scale;
        Vec3 hypot = new Vec3(flagHeight, flagWidth);
        Vec3 stem = new Vec3(0.7f * scale);
        int flagCount = (int)(planetRadius / 15.0f);
        for (int i = 0; i < flagCount; ++i) {
            Vec3 stem1 = stem.rotateD(-45.0f + (float)i * 360.0f / (float)flagCount);
            Vec3 hypot1 = hypot.rotateD(-45.0f + (float)i * 360.0f / (float)flagCount);
            Vec3[] points1 = new Vec3[]{stem1, stem1.normalized().timesEquals(flagHeight).plusEquals(stem1), stem1.plus(hypot1), stem1.normalized().rotateD(90.0f).timesEquals(flagWidth).plusEquals(stem1)};
            temp = new UWBGL_PrimitivePolygon(points1);
            flag.add(temp);
            temp = new UWBGL_PrimitiveLine(new Vec3(), stem1);
            flag.add(temp);
        }
        flag.setBlending(true);
        flag.setFlatColor(UWBGL_Color.CLEAR);
        flag.setFillMode(UWBGL_EFillMode.Solid);
        flag.setShadeMode(UWBGL_EShadeMode.Flat);
        return flag;
    }

    public UWBGL_PrimitiveCircle getPlanetPrimitive() {
        return this.planet_circle;
    }

    public static Planet makeSun(Vec3 loc, float sunRadius, String texture, boolean useTextures) {
        Planet m_sun = new Planet("Sun", loc, sunRadius);
        m_sun.m_HasParent = true;
        m_sun.space_port = null;
        m_sun.planetWrapper = null;
        UWBGL_Color sunColor = UWBGL_Color.YELLOW;
        m_sun.planet_circle.setFlatColor(sunColor);
        m_sun.planet_circle.setShadingColor(sunColor.randomVariation(0.5f));
        m_sun.planet_circle.setShadeMode(UWBGL_EShadeMode.Gouraud);
        m_sun.planet_circle.setBlending(true);
        m_sun.setPrimitive(m_sun.planet_circle);
        if (useTextures && texture != null) {
            m_sun.setTextureFileName(texture);
            m_sun.planet_circle.setTexturing(true);
        }
        return m_sun;
    }

    public int getOwnerId() {
        return this.m_OwnedBy;
    }

    public void scaleMass(float s) {
        this.m_Mass *= s;
    }

    public void setTextureFileName(String texture) {
        if (texture != null) {
            this.planet_circle.setTextureFileName("./rec/" + texture);
        }
    }

    public void setTexturing(boolean t) {
        this.planet_circle.setTexturing(t);
    }

    @Override
    public void setFacingDirection(Vec3 dir) {
        throw new UnsupportedOperationException("Not supported yet.");
    }

    @Override
    public void update(float delta_t) {
        super.update(delta_t);
        if (this.planetWrapper != null) {
            this.planetWrapper.update(delta_t);
        }
        this.updateLandingProgress(delta_t);
        this.m_location_dirty = true;
        this.m_translated_bounds_low_dirty = true;
        this.m_translated_bounds_high_dirty = true;
    }

    @Override
    public float getRadius() {
        return this.planet_circle.getRadius();
    }

    public boolean hasSpacePort() {
        return this.space_port != null;
    }

    public UWBGL_Primitive getsSpacePort() {
        return this.space_port;
    }

    public boolean hasHitSpacePort(UWBGL_SceneNode root, ObeysForces other, UWBGL_DrawHelper helper) {
        UWBGL_BoundingVolume other_high;
        UWBGL_BoundingVolume sp_high;
        UWBGL_BoundingVolume other_low;
        if (!this.hasSpacePort()) {
            return false;
        }
        UWBGL_BoundingVolume sp_low = root.getPrimitiveBounds(UWBGL_ELevelOfDetail.Low, this.space_port, helper);
        return sp_low.intersects(other_low = other.getBounds(UWBGL_ELevelOfDetail.Low, helper)) && (sp_high = root.getPrimitiveBounds(UWBGL_ELevelOfDetail.High, this.space_port, helper)).intersects(other_high = other.getBounds(UWBGL_ELevelOfDetail.High, helper));
    }

    @Override
    public UWBGL_Color getColor() {
        return this.planet_circle.getFlatColor();
    }

    public void setLandingShip(Ship ship) {
        if (this.m_LandingShip == null) {
            this.m_LandingShip = ship;
            return;
        }
        if (ship.getOwnerId() == this.m_OwnedBy) {
            this.m_LandingShip = ship;
            this.m_TakingOwnershipTime = 0.0f;
            return;
        }
        if (ship == this.m_LandingShipPrevRound) {
            this.m_LandingShip = ship;
            return;
        }
    }

    private void updateLandingProgress(float delta_t) {
        if (this.m_LandingShip == null) {
            this.m_LandingShipPrevRound = null;
            this.m_TakingOwnershipTime = 0.0f;
            if (this.m_Flag != null) {
                this.m_Flag.setShadeMode(UWBGL_EShadeMode.Flat);
            }
            if (this.space_port != null) {
                this.space_port.setDrawBoundingVolume(false);
            }
            return;
        }
        if (this.m_LandingShip == this.m_LandingShipPrevRound) {
            if (this.m_LandingShip.getOwnerId() == this.m_OwnedBy && EscapePod.class.isInstance(this.m_LandingShip)) {
                this.m_BuildingNewShipTime += delta_t;
                this.m_LandingShip.setLife(this.m_LandingShip.getLifeMax() * this.m_BuildingNewShipTime / 8.0f);
                if (this.m_BuildingNewShipTime >= 8.0f) {
                    this.m_BuildingNewShipTime = 0.0f;
                    ((EscapePod)this.m_LandingShip).setChangeToShip(true);
                }
            } else if (this.m_LandingShip.getOwnerId() == this.m_OwnedBy) {
                this.m_LandingShip.translateLife(3.0f * delta_t);
            } else if (this.m_LandingShip.getOwnerId() != this.m_OwnedBy && !EscapePod.class.isInstance(this.m_LandingShip)) {
                this.m_TakingOwnershipTime += delta_t;
                if (this.m_TakingOwnershipTime >= 4.0f) {
                    this.m_TakingOwnershipTime = 0.0f;
                    this.m_OwnedBy = this.m_LandingShip.getOwnerId();
                    this.m_Flag.setShadeMode(UWBGL_EShadeMode.Flat);
                    this.m_Flag.setFlatColor(this.m_LandingShip.getBaseColor());
                } else {
                    this.m_Flag.setShadeMode(UWBGL_EShadeMode.Gouraud);
                    this.m_Flag.setShadingColor(this.m_LandingShip.getBaseColor().withIntensity(this.m_TakingOwnershipTime / 4.0f));
                }
            }
        }
        this.space_port.setDrawBoundingVolume(true);
        this.m_LandingShipPrevRound = this.m_LandingShip;
        this.m_LandingShip = null;
    }

    private void calcLocation() {
        Vec3 c;
        UWBGL_BoundingVolume bounds;
        this.m_location_cache = this.m_Orbits_Parent ? ((bounds = this.getTranslatedBounds(UWBGL_ELevelOfDetail.Low)) != null ? (c = bounds.getCenter()) : new Vec3()) : this.getTranslatedBounds(UWBGL_ELevelOfDetail.High).getCenter();
        this.m_location_dirty = false;
    }

    public UWBGL_BoundingVolume getTranslatedBounds(UWBGL_ELevelOfDetail lod) {
        if (lod == UWBGL_ELevelOfDetail.Low) {
            if (this.m_translated_bounds_low_dirty) {
                this.m_Translated_Low_Bounds = this.m_Orbits_Parent ? this.m_Parent.getPrimitiveBounds(UWBGL_ELevelOfDetail.Low, this.planet_circle, DRAW_HELPER) : this.getPrimitiveBounds(UWBGL_ELevelOfDetail.Low, this.planet_circle, DRAW_HELPER);
                this.m_translated_bounds_low_dirty = false;
            }
            return this.m_Translated_Low_Bounds;
        }
        if (this.m_translated_bounds_high_dirty) {
            this.m_Translated_High_Bounds = this.m_Orbits_Parent ? this.m_Parent.getPrimitiveBounds(UWBGL_ELevelOfDetail.High, this.planet_circle, DRAW_HELPER) : this.getPrimitiveBounds(UWBGL_ELevelOfDetail.High, this.planet_circle, DRAW_HELPER);
            this.m_translated_bounds_high_dirty = false;
        }
        return this.m_Translated_High_Bounds;
    }

    @Override
    public Vec3 getLocation() {
        if (this.m_location_dirty) {
            this.calcLocation();
        }
        return this.m_location_cache;
    }

    public static String getRandomImage() {
        File planetsFolder = new File("./rec/planets/");
        if (!planetsFolder.exists() || !planetsFolder.isDirectory()) {
            return null;
        }
        int bestIndex = -1;
        double bestRandom = -1.0;
        double randomSelection = Math.random();
        File[] potentialPlanets = planetsFolder.listFiles();
        for (int i = 0; i < potentialPlanets.length; ++i) {
            double rand;
            if (!potentialPlanets[i].isFile() || !(Math.abs((rand = Math.random()) - randomSelection) < Math.abs(randomSelection - bestRandom))) continue;
            bestIndex = i;
            bestRandom = rand;
        }
        if (bestIndex < 0) {
            return null;
        }
        return IMAGE_RESOURCE_DIR + potentialPlanets[bestIndex].getName();
    }

    @Override
    public float damageAmount(Vec3 relative_velocity) {
        return relative_velocity.length() * 0.01f;
    }
}

