/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 */
import java.awt.Color;
import java.awt.Graphics;
import javax.swing.GroupLayout;
import javax.swing.JPanel;

public class ThreePartScorePanel
extends JPanel {
    private Color m_OutlineColor = Color.black;
    private Color m_TotalColor = Color.gray;
    private Color m_LeftColor = Color.red;
    private Color m_RightColor = Color.blue;
    private Color m_ValueColor = Color.black;
    private int m_TotalScore = 10;
    private int m_LeftScore = 1;
    private int m_RightScore = 1;
    private boolean m_ShowValues = true;

    public ThreePartScorePanel() {
        this.initComponents();
    }

    public boolean setValues(int left, int center, int right) {
        boolean changed = this.m_LeftScore != left || this.m_RightScore != right || this.m_TotalScore != left + right + center;
        this.m_LeftScore = left;
        this.m_RightScore = right;
        this.m_TotalScore = left + right + center;
        if (this.m_TotalScore <= 0) {
            this.m_TotalScore = 1;
        }
        if (changed) {
            this.repaint();
        }
        return changed;
    }

    public void setLeftColor(Color c) {
        this.m_LeftColor = c;
    }

    public void setRightColor(Color c) {
        this.m_RightColor = c;
    }

    public void setCenterColor(Color c) {
        this.m_TotalColor = c;
    }

    @Override
    public void paintComponent(Graphics g) {
        int y;
        String data;
        g.setColor(this.m_TotalColor);
        g.fillRoundRect(0, 0, this.getWidth() - 1, this.getHeight() - 1, 5, 5);
        if (this.m_ShowValues && this.m_LeftScore + this.m_RightScore < this.m_TotalScore) {
            g.setColor(this.m_ValueColor);
            data = new String("" + (this.m_TotalScore - this.m_LeftScore - this.m_RightScore));
            float x = (float)(this.m_TotalScore - this.m_RightScore - this.m_LeftScore) / 2.0f + (float)this.m_LeftScore;
            x = (float)((double)x * ((double)this.getWidth() / (double)this.m_TotalScore));
            y = (int)((double)this.getHeight() / 2.0);
            g.drawChars(data.toCharArray(), 0, data.length(), (int)x, y);
        }
        g.setColor(this.m_LeftColor);
        g.fillRoundRect(0, 0, this.m_LeftScore * this.getWidth() / this.m_TotalScore, this.getHeight(), 5, 5);
        if (this.m_ShowValues && this.m_LeftScore > 0) {
            g.setColor(this.m_ValueColor);
            data = new String("" + this.m_LeftScore);
            int x = (int)((double)this.m_LeftScore / 2.0 * ((double)this.getWidth() / (double)this.m_TotalScore));
            y = (int)((double)this.getHeight() / 2.0);
            g.drawChars(data.toCharArray(), 0, data.length(), x, y);
        }
        g.setColor(this.m_RightColor);
        g.fillRoundRect(this.getWidth() - this.m_RightScore * this.getWidth() / this.m_TotalScore, 0, this.getWidth() - 1, this.getHeight() - 1, 5, 5);
        if (this.m_ShowValues && this.m_RightScore > 0) {
            g.setColor(this.m_ValueColor);
            data = new String("" + this.m_RightScore);
            int x = (int)(((double)this.m_RightScore / 2.0 + (double)this.m_TotalScore - (double)this.m_RightScore) * ((double)this.getWidth() / (double)this.m_TotalScore));
            y = (int)((double)this.getHeight() / 2.0);
            g.drawChars(data.toCharArray(), 0, data.length(), x, y);
        }
        g.setColor(this.m_OutlineColor);
        g.drawRoundRect(0, 0, this.getWidth() - 1, this.getHeight() - 1, 5, 5);
    }

    private void initComponents() {
        GroupLayout layout = new GroupLayout(this);
        this.setLayout(layout);
        layout.setHorizontalGroup(layout.createParallelGroup(GroupLayout.Alignment.LEADING).addGap(0, 266, Short.MAX_VALUE));
        layout.setVerticalGroup(layout.createParallelGroup(GroupLayout.Alignment.LEADING).addGap(0, 98, Short.MAX_VALUE));
    }
}

