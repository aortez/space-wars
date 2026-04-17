/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 * 
 * Could not load the following classes:
 *  javax.media.opengl.GL
 */
import UWBGL_JavaOpenGL.UWBGL_Color;
import UWBGL_JavaOpenGL.UWBGL_DrawHelper;
import UWBGL_JavaOpenGL.UWBGL_ELevelOfDetail;
import UWBGL_JavaOpenGL.UWBGL_Xform;
import UWBGL_JavaOpenGL.math3d.Vec3;
import javax.media.opengl.GL;

public class BGStarField
implements BasicEntity {
    private static float COLOR_ROTATE_RATE = 0.02f;
    private static float COLOR_ROTATE_RANGE = 0.2f;
    private int star_count;
    private float area_filled;
    private float total_area;
    private boolean m_visible;
    private UWBGL_Color m_color;
    private float color_theta = 1.0f;
    private final int MAX_STARS = 100000;
    private float[] star_y = new float[300000];
    private float[] star_x = new float[300000];

    BGStarField(Vec3 center, float radius, float density) {
        this.m_color = UWBGL_Color.scale255(255.0f, 150.0f + (float)Math.random() * 100.0f, 150.0f + (float)Math.random() * 100.0f).withIntensity((float)(Math.random() * 0.5 + 0.5));
        this.total_area = (float)Math.PI * radius * radius;
        while (this.area_filled / this.total_area < density) {
            float angle = (float)(Math.random() * 2.0 * Math.PI);
            float rad = (float)Math.random() * radius;
            float x = (float)Math.cos(angle) * rad + center.x;
            float y = (float)Math.sin(angle) * rad + center.y;
            this.addStar(x, y, 0.9f, (float)Math.pow(Math.random(), 3.0) * 0.5f + 1.2f, 3);
            if (this.star_count < 100000) continue;
            return;
        }
    }

    private void addStar(float x, float y, float z, float size, int number_of_points) {
        float rotation = (float)Math.random() * (float)Math.PI;
        for (int i = 0; i < 3; ++i) {
            this.star_x[this.star_count * 3 + i] = (float)Math.cos((double)rotation + Math.PI * 2 / (double)number_of_points * (double)i) * size + x;
            this.star_y[this.star_count * 3 + i] = (float)Math.sin((double)rotation + Math.PI * 2 / (double)number_of_points * (double)i) * size + y;
        }
        ++this.star_count;
        this.area_filled = (float)((double)this.area_filled + Math.PI * 2 * (double)size);
    }

    private void redShift(UWBGL_Color c, float i) {
        float blue_shift = c.blue * i;
        c.blue -= blue_shift;
        float green_shift = c.green * i;
        c.green -= green_shift;
        c.red += blue_shift + green_shift;
    }

    @Override
    public UWBGL_Xform getXform() {
        throw new UnsupportedOperationException("Not supported yet.");
    }

    @Override
    public void draw(GL gl, UWBGL_ELevelOfDetail lod, UWBGL_DrawHelper helper) {
        gl.glPolygonMode(1032, 6914);
        if (this.m_visible) {
            UWBGL_Color c = this.m_color.withIntensity(1.0f - COLOR_ROTATE_RANGE * 0.5f + (float)Math.sin(this.color_theta) * COLOR_ROTATE_RANGE);
            this.color_theta += COLOR_ROTATE_RATE;
            gl.glColor3f(c.red, c.blue, c.green);
            gl.glBegin(4);
            int max = this.star_count * 3;
            for (int s = 0; s < max; ++s) {
                gl.glVertex3f(this.star_x[s], this.star_y[s], 0.9f);
            }
            gl.glEnd();
        }
    }

    @Override
    public void update(float time_delta) {
        throw new UnsupportedOperationException("Not supported yet.");
    }

    @Override
    public boolean visible() {
        return this.m_visible;
    }

    @Override
    public void setVisible(boolean v) {
        this.m_visible = v;
    }

    @Override
    public Vec3 getLocation() {
        throw new UnsupportedOperationException("Not supported yet.");
    }

    @Override
    public UWBGL_Color getColor() {
        throw new UnsupportedOperationException("Not supported yet.");
    }
}

