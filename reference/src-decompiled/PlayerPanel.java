/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 */
import javax.swing.GroupLayout;
import javax.swing.JLabel;
import javax.swing.JPanel;
import javax.swing.JProgressBar;
import javax.swing.LayoutStyle;

public class PlayerPanel
extends JPanel {
    private Player m_Player;
    private JLabel l_PlayerNumber;
    private JLabel l_ShipHealth;
    private JLabel m_PlayerName;
    private JProgressBar m_ShipHealth;

    public PlayerPanel() {
        this.initComponents();
    }

    public PlayerPanel(Player p) {
        this();
        this.setPlayer(p);
        this.updateDisplay();
    }

    public void setPlayer(Player p) {
        this.m_Player = p;
        this.l_PlayerNumber.setText("Player " + (p.getId() + 1) + ":");
        this.m_PlayerName.setText(p.getName());
        this.m_ShipHealth.setMaximum((int)this.m_Player.getShip().getLifeMax());
    }

    public void updateDisplay() {
        if (this.m_Player == null) {
            return;
        }
        if (this.m_Player.getShip().getLifeMax() != (float)this.m_ShipHealth.getMaximum()) {
            this.m_ShipHealth.setMaximum((int)this.m_Player.getShip().getLifeMax());
        }
        this.m_ShipHealth.setValue((int)this.m_Player.getShip().getLife());
    }

    private void initComponents() {
        this.l_PlayerNumber = new JLabel();
        this.m_PlayerName = new JLabel();
        this.l_ShipHealth = new JLabel();
        this.m_ShipHealth = new JProgressBar();
        this.l_PlayerNumber.setText("Player #: ");
        this.m_PlayerName.setText("PlayerName");
        this.l_ShipHealth.setText("Ship Health: ");
        GroupLayout layout = new GroupLayout(this);
        this.setLayout(layout);
        layout.setHorizontalGroup(layout.createParallelGroup(GroupLayout.Alignment.LEADING).addGroup(layout.createSequentialGroup().addContainerGap().addGroup(layout.createParallelGroup(GroupLayout.Alignment.LEADING).addComponent(this.l_ShipHealth).addComponent(this.l_PlayerNumber)).addGap(13, 13, 13).addGroup(layout.createParallelGroup(GroupLayout.Alignment.LEADING).addComponent(this.m_ShipHealth, -1, 156, Short.MAX_VALUE).addComponent(this.m_PlayerName, -1, 156, Short.MAX_VALUE))));
        layout.setVerticalGroup(layout.createParallelGroup(GroupLayout.Alignment.LEADING).addGroup(layout.createSequentialGroup().addContainerGap().addGroup(layout.createParallelGroup(GroupLayout.Alignment.BASELINE).addComponent(this.l_PlayerNumber).addComponent(this.m_PlayerName)).addPreferredGap(LayoutStyle.ComponentPlacement.RELATED).addGroup(layout.createParallelGroup(GroupLayout.Alignment.LEADING, false).addComponent(this.m_ShipHealth, -1, -1, Short.MAX_VALUE).addComponent(this.l_ShipHealth, -1, -1, Short.MAX_VALUE)).addContainerGap()));
    }
}

