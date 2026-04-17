/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 */
package UWBGL_JavaOpenGL;

import UWBGL_JavaOpenGL.math3d.Vec3;
import java.awt.Color;
import java.awt.Dimension;
import java.awt.Graphics;
import java.awt.Graphics2D;
import java.awt.event.MouseAdapter;
import java.awt.event.MouseEvent;
import java.awt.event.MouseMotionAdapter;
import javax.swing.JComponent;

public class UWBGL_Dial
extends JComponent {
    private int m_MinValue;
    private int m_MaxValue;
    private int m_Radius;
    private float m_NextAngle;
    private static float ORRIENTATION_OFFSET = 1.5707964f;

    public UWBGL_Dial() {
        this(0, 100, 0);
    }

    public UWBGL_Dial(int min, int max, int value) {
        this.setMinimum(min);
        this.setMaximum(max);
        this.setValue(value);
        this.setForeground(new Color(240, 240, 240));
        this.setBackground(new Color(240, 240, 240));
        this.setPreferredSize(new Dimension(100, 100));
        this.addMouseListener(new MouseAdapter(){

            @Override
            public void mousePressed(MouseEvent evt) {
                UWBGL_Dial.this.spin(evt);
            }
        });
        this.addMouseMotionListener(new MouseMotionAdapter(){

            @Override
            public void mouseDragged(MouseEvent evt) {
                UWBGL_Dial.this.spin(evt);
            }
        });
    }

    public void setMinimum(int min) {
        this.m_MinValue = min;
    }

    public void setMaximum(int max) {
        this.m_MaxValue = max;
    }

    public void setValue(int value) {
        this.m_NextAngle = (float)(value - this.m_MinValue) * ((float)Math.PI * 2) / (float)(this.m_MaxValue - this.m_MinValue) % ((float)Math.PI * 2);
    }

    public int getValue() {
        float output = (float)((double)(this.m_NextAngle * (float)(this.m_MaxValue - this.m_MinValue)) / (Math.PI * 2) + (double)this.m_MinValue);
        if (output < (float)this.m_MinValue) {
            output += (float)(this.m_MaxValue - this.m_MinValue);
        }
        if (output > (float)this.m_MaxValue) {
            output -= (float)(this.m_MaxValue - this.m_MinValue);
        }
        return (int)output;
    }

    public Vec3 getCenter() {
        float radius = (float)((double)Math.min(this.getSize().width, this.getSize().height) / 2.0);
        return new Vec3(radius, radius);
    }

    protected void spin(MouseEvent evt) {
        Vec3 mouse = new Vec3(evt.getX(), evt.getY());
        Vec3 dir = mouse.minusEquals(this.getCenter()).normalizedEquals();
        float angle = dir.angleRadians();
        this.m_NextAngle = angle - ORRIENTATION_OFFSET;
    }

    @Override
    public void paintComponent(Graphics g) {
        super.paintComponent(g);
        Graphics2D g2 = (Graphics2D)g;
        if (this.isOpaque()) {
            g.setColor(this.getBackground());
            g.fillRect(0, 0, this.getSize().width, this.getSize().height);
        }
        this.m_Radius = (int)((double)Math.min(this.getSize().width, this.getSize().height) / 2.0);
        g2.setColor(this.getBackground());
        this.draw3DCircle(g2, 0, 0, this.m_Radius, true);
        int knobRadius = this.m_Radius / 7;
        Vec3 knobCenter = Vec3.normFromRadians(-this.m_NextAngle - ORRIENTATION_OFFSET).timesEquals((float)knobRadius * 5.5f);
        this.draw3DCircle(g2, (int)knobCenter.x + this.m_Radius - knobRadius, (int)knobCenter.y + this.m_Radius - knobRadius, knobRadius, false);
    }

    private void draw3DCircle(Graphics g, int x, int y, int radius, boolean raised) {
        Color foreground = this.getForeground();
        Color light = foreground.brighter();
        Color dark = foreground.darker();
        g.setColor(foreground);
        g.fillOval(x, y, radius * 2, radius * 2);
        g.setColor(raised ? light : dark);
        g.drawArc(x, y, radius * 2, radius * 2, 45, 180);
        g.setColor(raised ? dark : light);
        g.drawArc(x, y, radius * 2, radius * 2, 225, 180);
    }
}

