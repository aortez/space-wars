/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 * 
 * Could not load the following classes:
 *  javax.media.opengl.GL
 */
package UWBGL_JavaOpenGL;

import java.awt.Color;
import javax.media.opengl.GL;

public class UWBGL_Color {
    public static final UWBGL_Color BLACK = new UWBGL_Color(0.0f, 0.0f, 0.0f);
    public static final UWBGL_Color WHITE = new UWBGL_Color(1.0f, 1.0f, 1.0f);
    public static final UWBGL_Color YELLOW = new UWBGL_Color(Color.YELLOW);
    public static final UWBGL_Color CLEAR = new UWBGL_Color(0.0f, 0.0f, 0.0f, 0.0f);
    public static final UWBGL_Color RED = new UWBGL_Color(Color.RED);
    public static final UWBGL_Color BLUE = new UWBGL_Color(Color.BLUE);
    public static final UWBGL_Color GREEN = new UWBGL_Color(Color.GREEN);
    public static final UWBGL_Color DIM_GREY = UWBGL_Color.scale255(100.0f, 100.0f, 100.0f);
    public float red;
    public float green;
    public float blue;
    public float alpha;

    public UWBGL_Color(UWBGL_Color color) {
        this.red = color.red;
        this.blue = color.blue;
        this.green = color.green;
        this.alpha = color.alpha;
    }

    public UWBGL_Color(float red, float green, float blue, float alpha) {
        this.red = red;
        this.green = green;
        this.blue = blue;
        this.alpha = alpha;
    }

    public UWBGL_Color(float red, float green, float blue) {
        this(red, green, blue, 1.0f);
    }

    public UWBGL_Color(Color c) {
        this(c.getRed() / 255, c.getGreen() / 255, c.getBlue() / 255, c.getAlpha() / 255);
    }

    public static UWBGL_Color scale255(float r, float g, float b) {
        return new UWBGL_Color(r / 255.0f, g / 255.0f, b / 255.0f);
    }

    public static UWBGL_Color scale255(float r, float g, float b, float a) {
        return new UWBGL_Color(r / 255.0f, g / 255.0f, b / 255.0f, a / 255.0f);
    }

    public Color getJavaColor() {
        return new Color(this.red, this.green, this.blue, this.alpha);
    }

    public void applyTo(GL gl) {
        gl.glColor4f(this.red, this.green, this.blue, this.alpha);
    }

    public float red() {
        return this.red * 255.0f;
    }

    public float green() {
        return this.green * 255.0f;
    }

    public float blue() {
        return this.blue * 255.0f;
    }

    public float alpha() {
        return this.alpha * 255.0f;
    }

    public float redPercent() {
        return this.red;
    }

    public float greenPercent() {
        return this.green;
    }

    public float bluePercent() {
        return this.blue;
    }

    public float alphaPercent() {
        return this.alpha;
    }

    public UWBGL_Color randomVariation(float var) {
        return new UWBGL_Color(((float)Math.random() - 0.5f) * var + this.red, ((float)Math.random() - 0.5f) * var + this.green, ((float)Math.random() - 0.5f) * var + this.blue);
    }

    public UWBGL_Color withIntensity(float i) {
        return new UWBGL_Color(this.red * i, this.green * i, this.blue * i, this.alpha * i);
    }

    public static UWBGL_Color random() {
        return new UWBGL_Color((float)Math.random(), (float)Math.random(), (float)Math.random());
    }

    public String toString() {
        return "Color R:" + this.red + " G:" + this.green + " B:" + this.blue + " A:" + this.alpha;
    }
}

