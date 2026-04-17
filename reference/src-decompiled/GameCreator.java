/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 */
import UWBGL_JavaOpenGL.UWBGL_Color;
import UWBGL_JavaOpenGL.Util.UWBGL_Util;
import java.awt.Color;
import java.awt.Component;
import java.awt.Dimension;
import java.awt.Font;
import java.awt.Frame;
import java.awt.Toolkit;
import java.awt.event.ActionEvent;
import java.awt.event.ActionListener;
import java.awt.event.MouseAdapter;
import java.awt.event.MouseEvent;
import javax.swing.BorderFactory;
import javax.swing.GroupLayout;
import javax.swing.JButton;
import javax.swing.JCheckBox;
import javax.swing.JColorChooser;
import javax.swing.JDialog;
import javax.swing.JLabel;
import javax.swing.JPanel;
import javax.swing.JSlider;
import javax.swing.JTextField;
import javax.swing.LayoutStyle;
import javax.swing.event.ChangeEvent;
import javax.swing.event.ChangeListener;
import org.netbeans.lib.awtextra.AbsoluteConstraints;
import org.netbeans.lib.awtextra.AbsoluteLayout;

public class GameCreator
extends JDialog
implements Common {
    private Model m_NewModel = null;
    private JLabel jLabel1;
    private JLabel jLabel2;
    private JLabel jLabel3;
    private JLabel jLabel4;
    private JLabel jLabel5;
    private JPanel jPanel1;
    private JLabel l_Color1;
    private JLabel l_Color2;
    private JLabel l_FPS;
    private JLabel l_Health1;
    private JLabel l_Health2;
    private JLabel l_Name1;
    private JLabel l_Name2;
    private JLabel l_Player1;
    private JLabel l_Player2;
    private JLabel l_WorldSize;
    private JLabel l_WorldSizeEnormous;
    private JLabel l_WorldSizeTiny;
    private JSlider m_Astroids;
    private JButton m_CancelB;
    private JPanel m_ColorChooser1;
    private JPanel m_ColorChooser2;
    private JButton m_DeathmatchB;
    private JButton m_DefaultB;
    private JButton m_EternalB;
    private JSlider m_FPS;
    private JTextField m_FPSText;
    private JButton m_GenerateB;
    private JSlider m_Health1;
    private JSlider m_Health2;
    private JTextField m_HealthPer1;
    private JTextField m_HealthPer2;
    private JTextField m_Name1;
    private JTextField m_Name2;
    private JPanel m_Panel1;
    private JPanel m_Panel3;
    private JCheckBox m_Planets;
    private JCheckBox m_Sounds;
    private JCheckBox m_Starfield;
    private JButton m_TestFPSB;
    private JCheckBox m_Texture;
    private JSlider m_WordSize;

    public GameCreator(Frame parent) {
        super(parent, true);
        this.initComponents();
        this.setConfig(GameConfig.getDefaults());
        Dimension d = Toolkit.getDefaultToolkit().getScreenSize();
        this.setLocation(d.width / 2 - this.getWidth() / 2, d.height / 2 - this.getHeight() / 2);
        this.setVisible(true);
    }

    public Model getNewModel() {
        return this.m_NewModel;
    }

    public void setConfig(GameConfig config) {
        this.m_WordSize.setValue(config.universeRadius);
        this.m_Astroids.setValue((int)(config.astroidProbobility * 100.0f));
        this.m_Texture.setSelected(config.useTextures);
        this.m_Starfield.setSelected(config.useStarfield);
        this.m_Planets.setSelected(config.usePlanets);
        this.m_FPS.setValue(config.fps);
        this.m_FPSText.setText("" + config.fps);
        this.m_Sounds.setSelected(config.useSounds);
        this.m_Name1.setText(config.playerNames[0]);
        this.m_Name2.setText(config.playerNames[1]);
        this.m_Health1.setValue(config.playerHealthPer[0]);
        this.m_HealthPer1.setText(config.playerHealthPer[0] + "%");
        this.m_Health2.setValue(config.playerHealthPer[1]);
        this.m_HealthPer2.setText(config.playerHealthPer[1] + "%");
        this.m_ColorChooser1.setBackground(config.playerColors[0].getJavaColor());
        this.m_ColorChooser2.setBackground(config.playerColors[1].getJavaColor());
    }

    public GameConfig getConfig() {
        GameConfig config = GameConfig.getDefaults();
        config.universeRadius = this.m_WordSize.getValue();
        config.astroidProbobility = (float)this.m_Astroids.getValue() / 100.0f;
        config.useTextures = this.m_Texture.isSelected();
        config.useStarfield = this.m_Starfield.isSelected();
        config.usePlanets = this.m_Planets.isSelected();
        config.fps = this.m_FPS.getValue();
        config.useSounds = this.m_Sounds.isSelected();
        config.playerNames[0] = this.m_Name1.getText();
        config.playerNames[1] = this.m_Name2.getText();
        config.playerHealthPer[0] = this.m_Health1.getValue();
        config.playerHealthPer[1] = this.m_Health2.getValue();
        config.playerColors[0] = new UWBGL_Color(this.m_ColorChooser1.getBackground());
        config.playerColors[1] = new UWBGL_Color(this.m_ColorChooser2.getBackground());
        return config;
    }

    private void initComponents() {
        this.m_GenerateB = new JButton();
        this.m_CancelB = new JButton();
        this.m_WordSize = new JSlider();
        this.jLabel1 = new JLabel();
        this.l_WorldSize = new JLabel();
        this.l_WorldSizeTiny = new JLabel();
        this.l_WorldSizeEnormous = new JLabel();
        this.m_Panel1 = new JPanel();
        this.m_Health1 = new JSlider();
        this.l_Health1 = new JLabel();
        this.l_Name1 = new JLabel();
        this.l_Player1 = new JLabel();
        this.m_Name1 = new JTextField();
        this.m_HealthPer1 = new JTextField();
        this.l_Color1 = new JLabel();
        this.m_ColorChooser1 = new JPanel();
        this.jLabel3 = new JLabel();
        this.m_Panel3 = new JPanel();
        this.m_Health2 = new JSlider();
        this.l_Health2 = new JLabel();
        this.l_Name2 = new JLabel();
        this.l_Player2 = new JLabel();
        this.m_Name2 = new JTextField();
        this.m_HealthPer2 = new JTextField();
        this.l_Color2 = new JLabel();
        this.m_ColorChooser2 = new JPanel();
        this.m_Texture = new JCheckBox();
        this.m_Starfield = new JCheckBox();
        this.m_Astroids = new JSlider();
        this.jLabel2 = new JLabel();
        this.jLabel5 = new JLabel();
        this.m_Planets = new JCheckBox();
        this.jPanel1 = new JPanel();
        this.jLabel4 = new JLabel();
        this.m_DefaultB = new JButton();
        this.m_DeathmatchB = new JButton();
        this.m_EternalB = new JButton();
        this.l_FPS = new JLabel();
        this.m_FPS = new JSlider();
        this.m_FPSText = new JTextField();
        this.m_TestFPSB = new JButton();
        this.m_Sounds = new JCheckBox();
        this.setDefaultCloseOperation(2);
        this.getContentPane().setLayout(new AbsoluteLayout());
        this.m_GenerateB.setText("Generate");
        this.m_GenerateB.addActionListener(new ActionListener(){

            @Override
            public void actionPerformed(ActionEvent evt) {
                GameCreator.this.m_GenerateBActionPerformed(evt);
            }
        });
        this.getContentPane().add((Component)this.m_GenerateB, new AbsoluteConstraints(290, 400, -1, 30));
        this.m_CancelB.setText("Cancel");
        this.m_CancelB.addActionListener(new ActionListener(){

            @Override
            public void actionPerformed(ActionEvent evt) {
                GameCreator.this.m_CancelBActionPerformed(evt);
            }
        });
        this.getContentPane().add((Component)this.m_CancelB, new AbsoluteConstraints(380, 400, -1, 30));
        this.m_WordSize.setMaximum(10000);
        this.m_WordSize.setMinimum(300);
        this.m_WordSize.setValue(1200);
        this.getContentPane().add((Component)this.m_WordSize, new AbsoluteConstraints(90, 70, 210, -1));
        this.jLabel1.setFont(new Font("Tahoma", 0, 36));
        this.jLabel1.setText("New Game Options");
        this.getContentPane().add((Component)this.jLabel1, new AbsoluteConstraints(80, 10, -1, -1));
        this.l_WorldSize.setText("Universe Size");
        this.getContentPane().add((Component)this.l_WorldSize, new AbsoluteConstraints(10, 70, -1, -1));
        this.l_WorldSizeTiny.setText("Tiny");
        this.getContentPane().add((Component)this.l_WorldSizeTiny, new AbsoluteConstraints(100, 90, -1, -1));
        this.l_WorldSizeEnormous.setText("Enormous");
        this.getContentPane().add((Component)this.l_WorldSizeEnormous, new AbsoluteConstraints(240, 90, -1, -1));
        this.m_Panel1.setBorder(BorderFactory.createBevelBorder(0));
        this.m_Health1.setMaximum(500);
        this.m_Health1.setMinimum(1);
        this.m_Health1.setValue(100);
        this.m_Health1.addChangeListener(new ChangeListener(){

            @Override
            public void stateChanged(ChangeEvent evt) {
                GameCreator.this.m_Health1StateChanged(evt);
            }
        });
        this.l_Health1.setText("Health");
        this.l_Name1.setText("Name");
        this.l_Player1.setText("Player 1:");
        this.m_Name1.setColumns(10);
        this.m_Name1.setText("Player 1");
        this.m_HealthPer1.setColumns(4);
        this.m_HealthPer1.setText("100%");
        this.m_HealthPer1.addActionListener(new ActionListener(){

            @Override
            public void actionPerformed(ActionEvent evt) {
                GameCreator.this.m_HealthPer1ActionPerformed(evt);
            }
        });
        this.l_Color1.setText("Color");
        this.m_ColorChooser1.setBackground(new Color(255, 0, 0));
        this.m_ColorChooser1.addMouseListener(new MouseAdapter(){

            @Override
            public void mouseClicked(MouseEvent evt) {
                GameCreator.this.m_ColorChooser1MouseClicked(evt);
            }
        });
        GroupLayout m_ColorChooser1Layout = new GroupLayout(this.m_ColorChooser1);
        this.m_ColorChooser1.setLayout(m_ColorChooser1Layout);
        m_ColorChooser1Layout.setHorizontalGroup(m_ColorChooser1Layout.createParallelGroup(GroupLayout.Alignment.LEADING).addGap(0, 28, Short.MAX_VALUE));
        m_ColorChooser1Layout.setVerticalGroup(m_ColorChooser1Layout.createParallelGroup(GroupLayout.Alignment.LEADING).addGap(0, 25, Short.MAX_VALUE));
        GroupLayout m_Panel1Layout = new GroupLayout(this.m_Panel1);
        this.m_Panel1.setLayout(m_Panel1Layout);
        m_Panel1Layout.setHorizontalGroup(m_Panel1Layout.createParallelGroup(GroupLayout.Alignment.LEADING).addGroup(m_Panel1Layout.createSequentialGroup().addContainerGap().addGroup(m_Panel1Layout.createParallelGroup(GroupLayout.Alignment.LEADING).addGroup(m_Panel1Layout.createSequentialGroup().addGroup(m_Panel1Layout.createParallelGroup(GroupLayout.Alignment.LEADING).addGroup(m_Panel1Layout.createSequentialGroup().addGroup(m_Panel1Layout.createParallelGroup(GroupLayout.Alignment.LEADING).addComponent(this.l_Health1).addComponent(this.l_Name1)).addPreferredGap(LayoutStyle.ComponentPlacement.RELATED).addGroup(m_Panel1Layout.createParallelGroup(GroupLayout.Alignment.LEADING).addGroup(m_Panel1Layout.createSequentialGroup().addComponent(this.m_Health1, -2, 83, -2).addPreferredGap(LayoutStyle.ComponentPlacement.RELATED, -1, Short.MAX_VALUE).addComponent(this.m_HealthPer1, -2, 47, -2)).addComponent(this.m_Name1, -1, 137, Short.MAX_VALUE))).addComponent(this.l_Player1)).addGap(36, 36, 36)).addGroup(m_Panel1Layout.createSequentialGroup().addComponent(this.l_Color1).addGap(18, 18, 18).addComponent(this.m_ColorChooser1, -2, -1, -2).addContainerGap()))));
        m_Panel1Layout.setVerticalGroup(m_Panel1Layout.createParallelGroup(GroupLayout.Alignment.LEADING).addGroup(m_Panel1Layout.createSequentialGroup().addContainerGap().addComponent(this.l_Player1).addPreferredGap(LayoutStyle.ComponentPlacement.RELATED).addGroup(m_Panel1Layout.createParallelGroup(GroupLayout.Alignment.BASELINE).addComponent(this.l_Name1).addComponent(this.m_Name1, -2, -1, -2)).addPreferredGap(LayoutStyle.ComponentPlacement.RELATED).addGroup(m_Panel1Layout.createParallelGroup(GroupLayout.Alignment.LEADING).addComponent(this.l_Health1).addGroup(m_Panel1Layout.createParallelGroup(GroupLayout.Alignment.TRAILING).addComponent(this.m_HealthPer1, -2, -1, -2).addComponent(this.m_Health1, -2, -1, -2))).addPreferredGap(LayoutStyle.ComponentPlacement.RELATED).addGroup(m_Panel1Layout.createParallelGroup(GroupLayout.Alignment.LEADING).addComponent(this.m_ColorChooser1, -2, -1, -2).addComponent(this.l_Color1)).addContainerGap(26, Short.MAX_VALUE)));
        this.getContentPane().add((Component)this.m_Panel1, new AbsoluteConstraints(20, 240, 210, 150));
        this.jLabel3.setText("Astroids");
        this.getContentPane().add((Component)this.jLabel3, new AbsoluteConstraints(10, 120, -1, -1));
        this.m_Panel3.setBorder(BorderFactory.createBevelBorder(0));
        this.m_Health2.setMaximum(500);
        this.m_Health2.setMinimum(1);
        this.m_Health2.setValue(100);
        this.m_Health2.addChangeListener(new ChangeListener(){

            @Override
            public void stateChanged(ChangeEvent evt) {
                GameCreator.this.m_Health2StateChanged(evt);
            }
        });
        this.l_Health2.setText("Health");
        this.l_Name2.setText("Name");
        this.l_Player2.setText("Player 2:");
        this.m_Name2.setColumns(10);
        this.m_Name2.setText("Player 2");
        this.m_HealthPer2.setColumns(4);
        this.m_HealthPer2.setText("100%");
        this.m_HealthPer2.addActionListener(new ActionListener(){

            @Override
            public void actionPerformed(ActionEvent evt) {
                GameCreator.this.m_HealthPer2ActionPerformed(evt);
            }
        });
        this.l_Color2.setText("Color");
        this.m_ColorChooser2.setBackground(new Color(0, 255, 0));
        this.m_ColorChooser2.addMouseListener(new MouseAdapter(){

            @Override
            public void mouseClicked(MouseEvent evt) {
                GameCreator.this.m_ColorChooser2MouseClicked(evt);
            }
        });
        GroupLayout m_ColorChooser2Layout = new GroupLayout(this.m_ColorChooser2);
        this.m_ColorChooser2.setLayout(m_ColorChooser2Layout);
        m_ColorChooser2Layout.setHorizontalGroup(m_ColorChooser2Layout.createParallelGroup(GroupLayout.Alignment.LEADING).addGap(0, 28, Short.MAX_VALUE));
        m_ColorChooser2Layout.setVerticalGroup(m_ColorChooser2Layout.createParallelGroup(GroupLayout.Alignment.LEADING).addGap(0, 25, Short.MAX_VALUE));
        GroupLayout m_Panel3Layout = new GroupLayout(this.m_Panel3);
        this.m_Panel3.setLayout(m_Panel3Layout);
        m_Panel3Layout.setHorizontalGroup(m_Panel3Layout.createParallelGroup(GroupLayout.Alignment.LEADING).addGroup(m_Panel3Layout.createSequentialGroup().addContainerGap().addGroup(m_Panel3Layout.createParallelGroup(GroupLayout.Alignment.LEADING).addGroup(m_Panel3Layout.createSequentialGroup().addGroup(m_Panel3Layout.createParallelGroup(GroupLayout.Alignment.LEADING).addGroup(m_Panel3Layout.createSequentialGroup().addGroup(m_Panel3Layout.createParallelGroup(GroupLayout.Alignment.LEADING).addComponent(this.l_Health2).addComponent(this.l_Name2)).addPreferredGap(LayoutStyle.ComponentPlacement.RELATED).addGroup(m_Panel3Layout.createParallelGroup(GroupLayout.Alignment.LEADING).addGroup(m_Panel3Layout.createSequentialGroup().addComponent(this.m_Health2, -2, 83, -2).addPreferredGap(LayoutStyle.ComponentPlacement.RELATED, -1, Short.MAX_VALUE).addComponent(this.m_HealthPer2, -2, 47, -2)).addComponent(this.m_Name2, -1, 137, Short.MAX_VALUE))).addComponent(this.l_Player2)).addGap(36, 36, 36)).addGroup(m_Panel3Layout.createSequentialGroup().addComponent(this.l_Color2).addGap(18, 18, 18).addComponent(this.m_ColorChooser2, -2, -1, -2).addContainerGap()))));
        m_Panel3Layout.setVerticalGroup(m_Panel3Layout.createParallelGroup(GroupLayout.Alignment.LEADING).addGroup(m_Panel3Layout.createSequentialGroup().addContainerGap().addComponent(this.l_Player2).addPreferredGap(LayoutStyle.ComponentPlacement.RELATED).addGroup(m_Panel3Layout.createParallelGroup(GroupLayout.Alignment.BASELINE).addComponent(this.l_Name2).addComponent(this.m_Name2, -2, -1, -2)).addPreferredGap(LayoutStyle.ComponentPlacement.RELATED).addGroup(m_Panel3Layout.createParallelGroup(GroupLayout.Alignment.LEADING).addComponent(this.l_Health2).addGroup(m_Panel3Layout.createParallelGroup(GroupLayout.Alignment.TRAILING).addComponent(this.m_HealthPer2, -2, -1, -2).addComponent(this.m_Health2, -2, -1, -2))).addPreferredGap(LayoutStyle.ComponentPlacement.RELATED).addGroup(m_Panel3Layout.createParallelGroup(GroupLayout.Alignment.LEADING).addComponent(this.m_ColorChooser2, -2, -1, -2).addComponent(this.l_Color2)).addContainerGap(26, Short.MAX_VALUE)));
        this.getContentPane().add((Component)this.m_Panel3, new AbsoluteConstraints(230, 240, 210, 150));
        this.m_Texture.setText("Textures");
        this.getContentPane().add((Component)this.m_Texture, new AbsoluteConstraints(10, 170, -1, -1));
        this.m_Starfield.setText("Starfield");
        this.getContentPane().add((Component)this.m_Starfield, new AbsoluteConstraints(90, 170, -1, -1));
        this.m_Astroids.setMaximum(10000);
        this.m_Astroids.setMinimum(0);
        this.getContentPane().add((Component)this.m_Astroids, new AbsoluteConstraints(60, 120, 240, -1));
        this.jLabel2.setText("Little");
        this.getContentPane().add((Component)this.jLabel2, new AbsoluteConstraints(70, 140, -1, -1));
        this.jLabel5.setText("Lots");
        this.getContentPane().add((Component)this.jLabel5, new AbsoluteConstraints(270, 140, -1, -1));
        this.m_Planets.setText("Planets");
        this.getContentPane().add((Component)this.m_Planets, new AbsoluteConstraints(170, 170, -1, -1));
        this.jPanel1.setBorder(BorderFactory.createBevelBorder(0));
        this.jLabel4.setText("Presets:");
        this.m_DefaultB.setText("Default");
        this.m_DefaultB.addActionListener(new ActionListener(){

            @Override
            public void actionPerformed(ActionEvent evt) {
                GameCreator.this.m_DefaultBActionPerformed(evt);
            }
        });
        this.m_DeathmatchB.setText("Death Match");
        this.m_DeathmatchB.addActionListener(new ActionListener(){

            @Override
            public void actionPerformed(ActionEvent evt) {
                GameCreator.this.m_DeathmatchBActionPerformed(evt);
            }
        });
        this.m_EternalB.setText("Eternal");
        this.m_EternalB.addActionListener(new ActionListener(){

            @Override
            public void actionPerformed(ActionEvent evt) {
                GameCreator.this.m_EternalBActionPerformed(evt);
            }
        });
        GroupLayout jPanel1Layout = new GroupLayout(this.jPanel1);
        this.jPanel1.setLayout(jPanel1Layout);
        jPanel1Layout.setHorizontalGroup(jPanel1Layout.createParallelGroup(GroupLayout.Alignment.LEADING).addGroup(jPanel1Layout.createSequentialGroup().addContainerGap().addGroup(jPanel1Layout.createParallelGroup(GroupLayout.Alignment.TRAILING, false).addComponent(this.m_EternalB, GroupLayout.Alignment.LEADING, -1, -1, Short.MAX_VALUE).addComponent(this.jLabel4, GroupLayout.Alignment.LEADING).addComponent(this.m_DeathmatchB, GroupLayout.Alignment.LEADING, -1, -1, Short.MAX_VALUE).addComponent(this.m_DefaultB, GroupLayout.Alignment.LEADING, -1, -1, Short.MAX_VALUE)).addContainerGap(-1, Short.MAX_VALUE)));
        jPanel1Layout.setVerticalGroup(jPanel1Layout.createParallelGroup(GroupLayout.Alignment.LEADING).addGroup(jPanel1Layout.createSequentialGroup().addContainerGap().addComponent(this.jLabel4).addPreferredGap(LayoutStyle.ComponentPlacement.RELATED).addComponent(this.m_DefaultB).addPreferredGap(LayoutStyle.ComponentPlacement.RELATED).addComponent(this.m_DeathmatchB).addPreferredGap(LayoutStyle.ComponentPlacement.RELATED).addComponent(this.m_EternalB).addContainerGap(51, Short.MAX_VALUE)));
        this.getContentPane().add((Component)this.jPanel1, new AbsoluteConstraints(310, 60, 130, 180));
        this.l_FPS.setText("FPS:");
        this.getContentPane().add((Component)this.l_FPS, new AbsoluteConstraints(10, 210, -1, -1));
        this.m_FPS.setMaximum(150);
        this.m_FPS.setMinimum(10);
        this.m_FPS.setValue(60);
        this.m_FPS.addChangeListener(new ChangeListener(){

            @Override
            public void stateChanged(ChangeEvent evt) {
                GameCreator.this.m_FPSStateChanged(evt);
            }
        });
        this.getContentPane().add((Component)this.m_FPS, new AbsoluteConstraints(40, 210, 150, -1));
        this.m_FPSText.setColumns(3);
        this.m_FPSText.setText("60");
        this.m_FPSText.addActionListener(new ActionListener(){

            @Override
            public void actionPerformed(ActionEvent evt) {
                GameCreator.this.m_FPSTextActionPerformed(evt);
            }
        });
        this.getContentPane().add((Component)this.m_FPSText, new AbsoluteConstraints(190, 210, -1, -1));
        this.m_TestFPSB.setText("Test");
        this.m_TestFPSB.addActionListener(new ActionListener(){

            @Override
            public void actionPerformed(ActionEvent evt) {
                GameCreator.this.m_TestFPSBActionPerformed(evt);
            }
        });
        this.getContentPane().add((Component)this.m_TestFPSB, new AbsoluteConstraints(240, 210, -1, -1));
        this.m_Sounds.setText("Sounds");
        this.getContentPane().add((Component)this.m_Sounds, new AbsoluteConstraints(240, 170, -1, -1));
        this.pack();
    }

    private void m_ColorChooser1MouseClicked(MouseEvent evt) {
        Color c = JColorChooser.showDialog(this, "Pick a Color", this.m_ColorChooser1.getBackground());
        if (c != null) {
            this.m_ColorChooser1.setBackground(c);
        }
    }

    private void m_ColorChooser2MouseClicked(MouseEvent evt) {
        Color c = JColorChooser.showDialog(this, "Pick a Color", this.m_ColorChooser2.getBackground());
        if (c != null) {
            this.m_ColorChooser2.setBackground(c);
        }
    }

    private void m_CancelBActionPerformed(ActionEvent evt) {
        this.setVisible(false);
    }

    private void m_GenerateBActionPerformed(ActionEvent evt) {
        this.m_NewModel = new Model(this.getConfig());
        this.setVisible(false);
    }

    private void m_Health1StateChanged(ChangeEvent evt) {
        this.m_HealthPer1.setText(this.m_Health1.getValue() + "%");
    }

    private void m_Health2StateChanged(ChangeEvent evt) {
        this.m_HealthPer2.setText(this.m_Health2.getValue() + "%");
    }

    private void m_HealthPer1ActionPerformed(ActionEvent evt) {
        int value;
        String data = this.m_HealthPer1.getText();
        if (data.endsWith("%")) {
            data = data.substring(0, data.length() - 1);
        }
        if ((value = Integer.parseInt(data)) > 500) {
            value = 500;
        }
        if (value < 1) {
            value = 1;
        }
        this.m_HealthPer1.setText(value + "%");
        this.m_Health1.setValue(value);
    }

    private void m_HealthPer2ActionPerformed(ActionEvent evt) {
        int value;
        String data = this.m_HealthPer2.getText();
        if (data.endsWith("%")) {
            data = data.substring(0, data.length() - 1);
        }
        if ((value = Integer.parseInt(data)) > 500) {
            value = 500;
        }
        if (value < 1) {
            value = 1;
        }
        this.m_HealthPer2.setText(value + "%");
        this.m_Health2.setValue(value);
    }

    private void m_DeathmatchBActionPerformed(ActionEvent evt) {
        this.setConfig(GameConfig.deathMatch());
    }

    private void m_DefaultBActionPerformed(ActionEvent evt) {
        this.setConfig(GameConfig.getDefaults());
    }

    private void m_EternalBActionPerformed(ActionEvent evt) {
        this.setConfig(GameConfig.eternal());
    }

    private void m_FPSStateChanged(ChangeEvent evt) {
        this.m_FPSText.setText("" + this.m_FPS.getValue());
    }

    private void m_FPSTextActionPerformed(ActionEvent evt) {
        String data = this.m_FPSText.getText();
        int value = Integer.parseInt(data);
        if (value < 10) {
            value = 10;
        }
        if (value > 150) {
            value = 150;
        }
        this.m_FPS.setValue(value);
        this.m_FPSText.setText("" + value);
    }

    private void m_TestFPSBActionPerformed(ActionEvent evt) {
        this.m_FPSText.setText("...");
        Model testModel = new Model(this.getConfig());
        FinalDlg testDlg = new FinalDlg(testModel, false);
        float result = testDlg.benchmark(750);
        this.m_FPSText.setText(UWBGL_Util.truncateStr("" + result, 0));
        this.m_FPSTextActionPerformed(null);
    }
}

