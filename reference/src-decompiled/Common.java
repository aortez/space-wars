/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 */
import UWBGL_JavaOpenGL.UWBGL_DrawHelper;
import UWBGL_JavaOpenGL.UWBGL_DrawHelperOGL;

public interface Common {
    public static final boolean BENCHMARK = false;
    public static final UWBGL_DrawHelper DRAW_HELPER = new UWBGL_DrawHelperOGL();
    public static final boolean PLAYER_GRAVITY = true;
    public static final int BENCHMARK_FRAMES = 750;
    public static final double MIN_VIEW_SIZE = 15.0;
    public static final int STARTUP_ZOOM_FACTOR = 800;
    public static final int PARTICLE_GRAV_SKIP_LEVEL = 3;
    public static final int ASTEROID_GRAV_SKIP_LEVEL = 7;
    public static final float LASER_DAMAGE = 10.0f;
    public static final float LASER_VELOCITY = 50.0f;
    public static final float LASER_MASS = 0.5f;
    public static final float PARTICLE_MASS = 0.1f;
    public static final float DEFAULT_PARTICLE_FADE_RATE = 0.19607843f;
    public static final float CANNON_DAMAGE = 0.1f;
    public static final float CANNON_COOL_DOWN_TIME = 0.5f;
    public static final float CANNON_VELOCITY = 300.0f;
    public static final float SHELL_LIFE = 1.0f;
    public static final float ASTEROID_DAMAGE = 0.01f;
    public static final float DEFAULT_ASTEROID_SIZE = 5.0f;
    public static final float PROB_HUGE_ASTEROID = 0.98f;
    public static final int PLAYER1 = 0;
    public static final int PLAYER2 = 1;
    public static final int PLAYER_COUNT = 2;
    public static final float REALLY_SMALL = 1.0E-7f;
    public static final float PI = (float)Math.PI;
    public static final int PLANET_MASS_SCALAR = 750;
    public static final float GRAVITY = 0.005f;
    public static final float PLANET_DAMAGE_SCALAR = 0.01f;
    public static final float COLLISION_TRANSLATION_SCALAR = 1.0f;
    public static final float DEFAULT_ELASTICITY = 0.9f;
    public static final float PLANET_ELASTICITY = 0.5f;
    public static final float PARTICLE_VELOCITY_SCALAR = 20.0f;
    public static final float FLACK_SCALAR = 1.0f;
    public static final float EXHAUST_MASS = 0.5f;
    public static final String RESOURCE_FOLDER = "./rec/";
    public static final String SOUNDS_FOLDER = "./rec/sounds/";
    public static final float Z_BGSTARS = 0.9f;
    public static final float Z_LASER = 0.8f;
    public static final float Z_PLANETS = 0.7f;
    public static final float Z_PARTICLES = -0.9f;
    public static final float Z_SHIP = -0.8f;
}

