/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 */
import UWBGL_JavaOpenGL.UWBGL_Color;

public class GameConfig
implements Common {
    public static final int MAX_UNIVERSE_RADIUS = 10000;
    public static final int MIN_UNIVERSE_RADIUS = 300;
    public static final float MAX_ASTEROID_PROB = 100.0f;
    public static final float MIN_ASTEROID_PROB = 0.0f;
    public static final int MAX_PLAYER_HEALTH_PER = 500;
    public static final int MIN_PLAYER_HEALTH_PER = 1;
    public static final int MAX_FPS = 150;
    public static final int MIN_FPS = 10;
    public static final int STARTUP_UNIVERSE_RADIUS = 1200;
    public static final float STARTUP_ASTEROID_PROB = 20.0f;
    public static final int STARTUP_PLAYER_HEALTH_PER = 100;
    public static final int STARTUP_FPS = 60;
    public static final boolean STARTUP_USE_TEXTURES = true;
    public static final boolean STARTUP_USE_STARFIELD = true;
    public static final boolean STARTUP_USE_PLANETS = true;
    public static final boolean STARTUP_USE_SOUNDS = true;
    static final UWBGL_Color[] STARTUP_PLAYER_COLORS = new UWBGL_Color[]{UWBGL_Color.RED, UWBGL_Color.GREEN};
    public static final GameConfig STARTUP_CONFIG = GameConfig.getDefaults();
    public int universeRadius;
    public String[] playerNames = new String[2];
    public int[] playerHealthPer = new int[2];
    public UWBGL_Color[] playerColors = new UWBGL_Color[2];
    public float astroidProbobility;
    public boolean useTextures;
    public boolean useStarfield;
    public boolean usePlanets;
    public int fps;
    public boolean useSounds;

    public static GameConfig getDefaults() {
        GameConfig defaults = new GameConfig();
        defaults.universeRadius = 1200;
        defaults.astroidProbobility = 20.0f;
        defaults.useTextures = true;
        defaults.useStarfield = true;
        defaults.usePlanets = true;
        defaults.fps = 60;
        defaults.useSounds = true;
        defaults.playerNames[0] = "Player 1";
        defaults.playerNames[1] = "Player 2";
        defaults.playerHealthPer[0] = 100;
        defaults.playerHealthPer[1] = 100;
        defaults.playerColors[0] = STARTUP_PLAYER_COLORS[0];
        defaults.playerColors[1] = STARTUP_PLAYER_COLORS[1];
        return defaults;
    }

    public static GameConfig deathMatch() {
        GameConfig deathmatch = GameConfig.getDefaults();
        deathmatch.usePlanets = false;
        deathmatch.astroidProbobility = 100.0f;
        deathmatch.universeRadius = 300;
        deathmatch.playerHealthPer[0] = 50;
        deathmatch.playerHealthPer[1] = 50;
        return deathmatch;
    }

    public static GameConfig eternal() {
        GameConfig eternal = GameConfig.getDefaults();
        eternal.astroidProbobility = 100.0f;
        eternal.universeRadius = 10000;
        eternal.playerHealthPer[0] = 500;
        eternal.playerHealthPer[1] = 500;
        eternal.useStarfield = false;
        eternal.useTextures = false;
        return eternal;
    }
}

