/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 * 
 * Could not load the following classes:
 *  javax.media.opengl.GL
 *  javax.media.opengl.GLAutoDrawable
 *  javax.media.opengl.GLEventListener
 */
package UWBGL_JavaOpenGL;

import UWBGL_JavaOpenGL.UWBGL_Color;
import UWBGL_JavaOpenGL.UWBGL_Displayer;
import UWBGL_JavaOpenGL.math3d.Vec3;
import java.awt.Component;
import java.awt.Dimension;
import java.awt.Point;
import java.awt.Rectangle;
import javax.media.opengl.GL;
import javax.media.opengl.GLAutoDrawable;
import javax.media.opengl.GLEventListener;

public class UWBGL_DisplayScalar
implements GLEventListener {
    private UWBGL_Color m_BG_Color;
    private UWBGL_Displayer m_Displayer;
    private Rectangle m_ViewBounds;
    private boolean m_ViewBoundsDirty;
    private Component m_ViewComponent;
    private boolean m_MaintainAspect;
    private boolean m_CenterAspect;

    public UWBGL_DisplayScalar(Component viewComponent, Rectangle worldBounds, UWBGL_Color bgcolor, UWBGL_Displayer disp) {
        this.m_BG_Color = bgcolor;
        this.m_Displayer = disp;
        this.m_ViewBounds = new Rectangle(worldBounds);
        this.m_ViewComponent = viewComponent;
        this.m_ViewBoundsDirty = true;
        this.m_MaintainAspect = true;
        this.m_CenterAspect = true;
    }

    public void init(GLAutoDrawable drawable) {
        GL gl = drawable.getGL();
        gl.glEnable(2929);
        gl.glDepthFunc(513);
        gl.setSwapInterval(0);
        gl.glClearColor(this.m_BG_Color.redPercent(), this.m_BG_Color.greenPercent(), this.m_BG_Color.bluePercent(), this.m_BG_Color.alphaPercent());
        gl.glShadeModel(7425);
        gl.glEnable(2848);
        gl.glEnable(3042);
        gl.glBlendFunc(770, 771);
        gl.glHint(3154, 4354);
        gl.glEnable(32925);
    }

    public void display(GLAutoDrawable drawable) {
        GL gl = drawable.getGL();
        if (this.m_ViewBoundsDirty) {
            this.redoBounds(drawable);
        }
        gl.glClear(16640);
        this.m_Displayer.display(drawable);
    }

    public void reshape(GLAutoDrawable drawable, int x, int y, int width, int height) {
        this.m_ViewBoundsDirty = true;
    }

    private Vec3 adjustForAspect(Vec3 p, boolean center) {
        float viewAspect = (float)this.m_ViewBounds.width / (float)this.m_ViewBounds.height;
        Dimension displaySize = this.m_ViewComponent.getSize();
        float outerAspect = (float)displaySize.width / (float)displaySize.height;
        float x = p.x;
        float y = p.y;
        if (viewAspect >= outerAspect) {
            y = p.y * viewAspect / outerAspect;
            if (center) {
                y -= ((float)this.m_ViewBounds.height * viewAspect / outerAspect - (float)this.m_ViewBounds.height) / 2.0f;
            }
        } else {
            x = p.x * outerAspect / viewAspect;
            if (center) {
                x -= ((float)this.m_ViewBounds.width * outerAspect / viewAspect - (float)this.m_ViewBounds.width) / 2.0f;
            }
        }
        return new Vec3(x, y, 0.0f);
    }

    public Vec3 hardwareToNdc(Point hardware) {
        Dimension displaySize = this.m_ViewComponent.getSize();
        float x_NDC = (float)hardware.x / (float)displaySize.width;
        float y_NDC = (float)(displaySize.height - hardware.y) / (float)displaySize.height;
        Vec3 device = new Vec3(x_NDC, y_NDC, 0.0f);
        return device;
    }

    public Vec3 ndcToWorld(Vec3 device) {
        float x = device.x * (float)this.m_ViewBounds.width;
        float y = device.y * (float)this.m_ViewBounds.height;
        Vec3 world = new Vec3(x, y, 0.0f);
        if (this.m_MaintainAspect) {
            world = this.adjustForAspect(world, this.m_CenterAspect);
        }
        world = new Vec3(world.x + (float)this.m_ViewBounds.x, world.y + (float)this.m_ViewBounds.y, 0.0f);
        return world;
    }

    public Vec3 hardwareToWorld(Point hardware) {
        return this.ndcToWorld(this.hardwareToNdc(hardware));
    }

    public void displayChanged(GLAutoDrawable drawable, boolean modeChanged, boolean deviceChanged) {
    }

    public boolean isCentered() {
        return this.m_CenterAspect;
    }

    public void setCentered(boolean m_Center) {
        this.m_CenterAspect = m_Center;
    }

    public void setViewBounds(Vec3 min, Vec3 max) {
        if (this.m_ViewBounds.x != (int)min.x || this.m_ViewBounds.y != (int)min.y || this.m_ViewBounds.width != (int)(max.x - min.x) || this.m_ViewBounds.height != (int)(max.y - min.y)) {
            this.m_ViewBoundsDirty = true;
            this.m_ViewBounds.x = (int)min.x;
            this.m_ViewBounds.y = (int)min.y;
            this.m_ViewBounds.width = (int)(max.x - min.x);
            this.m_ViewBounds.height = (int)(max.y - min.y);
        }
    }

    public void setViewBounds(Rectangle r) {
        this.setViewBounds(new Vec3((float)r.getMinX(), (float)r.getMinY(), 0.0f), new Vec3((float)r.getMaxX(), (float)r.getMaxY(), 0.0f));
    }

    public Rectangle getViewBounds() {
        return this.m_ViewBounds;
    }

    private void redoBounds(GLAutoDrawable drawable) {
        GL gl = drawable.getGL();
        gl.glMatrixMode(5889);
        gl.glLoadIdentity();
        Vec3 size = new Vec3(this.m_ViewBounds.width, this.m_ViewBounds.height, 0.0f);
        if (this.m_MaintainAspect) {
            size = this.adjustForAspect(size, false);
        }
        gl.glScalef(2.0f / size.x, 2.0f / size.y, 1.0f);
        if (this.m_CenterAspect) {
            gl.glTranslatef(-((float)this.m_ViewBounds.getCenterX()), -((float)this.m_ViewBounds.getCenterY()), 0.0f);
        }
        this.m_ViewBoundsDirty = false;
    }
}

