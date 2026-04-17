/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 * 
 * Could not load the following classes:
 *  javax.media.opengl.GLAutoDrawable
 *  javax.media.opengl.GLCanvas
 *  javax.media.opengl.GLEventListener
 */
import UWBGL_JavaOpenGL.UWBGL_Color;
import UWBGL_JavaOpenGL.UWBGL_DisplayScalar;
import UWBGL_JavaOpenGL.UWBGL_Displayer;
import UWBGL_JavaOpenGL.Util.UWBGL_Timer;
import UWBGL_JavaOpenGL.Util.UWBGL_TimerListener;
import java.awt.Component;
import java.awt.Dimension;
import java.awt.EventQueue;
import java.awt.Rectangle;
import java.awt.Toolkit;
import java.awt.event.ActionEvent;
import java.awt.event.ActionListener;
import javax.media.opengl.GLAutoDrawable;
import javax.media.opengl.GLCanvas;
import javax.media.opengl.GLEventListener;
import javax.swing.GroupLayout;
import javax.swing.JButton;
import javax.swing.JFrame;
import javax.swing.JLabel;
import javax.swing.JOptionPane;
import javax.swing.JPanel;
import javax.swing.JSlider;
import javax.swing.JToggleButton;
import javax.swing.LayoutStyle;
import javax.swing.UIManager;
import javax.swing.UnsupportedLookAndFeelException;
import javax.swing.event.ChangeEvent;
import javax.swing.event.ChangeListener;
import javax.swing.plaf.metal.MetalLookAndFeel;
import javax.swing.plaf.metal.OceanTheme;

public class FinalDlg
extends JFrame
implements UWBGL_Displayer,
UWBGL_TimerListener,
Common {
    private static KeyboardInput keyboard = new KeyboardInput();
    public static final UWBGL_Color bg_color = UWBGL_Color.BLACK;
    private UWBGL_Timer timey;
    private static int m_Small_View_Skip;
    private static float m_RequestFocusCount;
    private Model m_Model;
    private UWBGL_DisplayScalar m_WorldCanvasScalar1;
    private UWBGL_DisplayScalar m_HeroCanvasScalar1;
    private UWBGL_DisplayScalar m_WorldCanvasScalar2;
    private UWBGL_DisplayScalar m_HeroCanvasScalar2;
    private GLCanvas m_HeroViewCanvas1;
    private GLCanvas m_HeroViewCanvas2;
    private JPanel m_MoreControlsPanel;
    private JButton m_NewGameB;
    private JToggleButton m_PauseB;
    private JPanel m_PlanetScorePanel;
    private JPanel m_PlayerPanel1;
    private JPanel m_PlayerPanel2;
    private GLCanvas m_WorldViewCanvas1;
    private GLCanvas m_WorldViewCanvas2;
    private JLabel m_ZoomLabel1;
    private JLabel m_ZoomLabel2;
    private JSlider m_ZoomSlider1;
    private JSlider m_ZoomSlider2;

    public FinalDlg(Model model, boolean start) {
        FinalDlg.setGUILook();
        this.m_Model = model;
        this.initComponents();
        Rectangle world_canvas_bounds1 = new Rectangle(this.m_WorldViewCanvas1.getBounds());
        this.m_WorldCanvasScalar1 = new UWBGL_DisplayScalar((Component)this.m_WorldViewCanvas1, this.m_Model.getWorldViewBounds(), bg_color, this);
        this.m_WorldViewCanvas1.addGLEventListener((GLEventListener)this.m_WorldCanvasScalar1);
        this.m_HeroCanvasScalar1 = new UWBGL_DisplayScalar((Component)this.m_HeroViewCanvas1, this.m_Model.getWorldViewBounds(), bg_color, this);
        this.m_HeroViewCanvas1.addGLEventListener((GLEventListener)this.m_HeroCanvasScalar1);
        this.m_HeroCanvasScalar1.setViewBounds(this.m_Model.getViewBounds(0).getMin(), this.m_Model.getViewBounds(0).getMax());
        double sliderValue = this.m_Model.getViewBounds(0).getSize();
        sliderValue = sliderValue / this.m_Model.getWorldViewHypot() * 800.0;
        this.m_ZoomSlider1.setValue((int)sliderValue);
        Rectangle world_canvas_bounds2 = new Rectangle(this.m_WorldViewCanvas2.getBounds());
        this.m_WorldCanvasScalar2 = new UWBGL_DisplayScalar((Component)this.m_WorldViewCanvas2, this.m_Model.getWorldViewBounds(), bg_color, this);
        this.m_WorldViewCanvas2.addGLEventListener((GLEventListener)this.m_WorldCanvasScalar2);
        this.m_HeroCanvasScalar2 = new UWBGL_DisplayScalar((Component)this.m_HeroViewCanvas2, this.m_Model.getWorldViewBounds(), bg_color, this);
        this.m_HeroViewCanvas2.addGLEventListener((GLEventListener)this.m_HeroCanvasScalar2);
        this.m_HeroCanvasScalar2.setViewBounds(this.m_Model.getViewBounds(1).getMin(), this.m_Model.getViewBounds(1).getMax());
        sliderValue = this.m_Model.getViewBounds(1).getSize();
        sliderValue = sliderValue / this.m_Model.getWorldViewHypot() * 800.0;
        this.m_ZoomSlider2.setValue((int)sliderValue);
        ThreePartScorePanel planetScore = (ThreePartScorePanel)this.m_PlanetScorePanel;
        planetScore.setLeftColor(this.m_Model.getPlayer(0).getColor().getJavaColor());
        planetScore.setRightColor(this.m_Model.getPlayer(1).getColor().getJavaColor());
        Dimension d = Toolkit.getDefaultToolkit().getScreenSize();
        this.setLocation(d.width / 2 - this.getWidth() / 2, d.height / 2 - this.getHeight() / 2);
        if (start) {
            this.timey = new UWBGL_Timer((int)(this.m_Model.getDeltaTime() * 1000.0f), this);
            this.setVisible(true);
        }
        this.m_MoreControlsPanel.addKeyListener(keyboard);
        this.m_MoreControlsPanel.requestFocus();
    }

    public float benchmark(int frames) {
        long start = System.currentTimeMillis();
        for (int i = 0; i < frames; ++i) {
            this.timerEvent();
        }
        long finish = System.currentTimeMillis();
        long duration = finish - start;
        float result = (float)frames / (float)duration * 1000.0f;
        System.out.println("$ Elapsed time: " + duration);
        System.out.println("$ Updates per second: " + result);
        return result;
    }

    @Override
    public void timerEvent() {
        keyboard.poll();
        this.handleKeys();
        this.m_Model.doPhysics();
        int winner = this.m_Model.gameOver();
        if (winner >= 0) {
            keyboard.clear();
            this.m_Model.setPause(true);
            int answer = JOptionPane.showConfirmDialog(this, "Game Over! " + this.m_Model.getPlayer(winner).getName() + " Wins!\n" + "Start a new game?", "Game Over!", 2);
            if (answer == 2) {
                System.exit(0);
            }
            this.m_NewGameBActionPerformed(null);
            this.m_MoreControlsPanel.requestFocus();
        }
        this.m_HeroCanvasScalar1.setViewBounds(this.m_Model.getViewBounds(0).getMin(), this.m_Model.getViewBounds(0).getMax());
        this.m_HeroCanvasScalar2.setViewBounds(this.m_Model.getViewBounds(1).getMin(), this.m_Model.getViewBounds(1).getMax());
        this.m_Model.cacheData();
        ((PlayerPanel)this.m_PlayerPanel1).updateDisplay();
        ((PlayerPanel)this.m_PlayerPanel2).updateDisplay();
        this.updatePlanetScore();
        this.m_Model.setStarFieldVisible(true);
        this.m_Model.setViewBoundsVisible(0, false);
        this.m_Model.setViewBoundsVisible(1, false);
        this.m_HeroViewCanvas1.display();
        this.m_HeroViewCanvas2.display();
        if (m_Small_View_Skip > 3) {
            this.m_Model.setStarFieldVisible(false);
            this.m_Model.setViewBoundsVisible(0, true);
            this.m_WorldViewCanvas1.display();
            this.m_Model.setViewBoundsVisible(1, true);
            this.m_Model.setViewBoundsVisible(0, false);
            this.m_WorldViewCanvas2.display();
            m_Small_View_Skip = 0;
        }
        ++m_Small_View_Skip;
        if ((m_RequestFocusCount += this.m_Model.getDeltaTime()) > 0.5f) {
            m_RequestFocusCount = 0.0f;
            this.m_MoreControlsPanel.requestFocus();
        }
    }

    private void initComponents() {
        this.m_HeroViewCanvas2 = new GLCanvas();
        this.m_WorldViewCanvas2 = new GLCanvas();
        this.m_MoreControlsPanel = new JPanel();
        this.m_HeroViewCanvas1 = new GLCanvas();
        this.m_WorldViewCanvas1 = new GLCanvas();
        this.m_ZoomSlider2 = new JSlider();
        this.m_ZoomLabel2 = new JLabel();
        this.m_ZoomLabel1 = new JLabel();
        this.m_ZoomSlider1 = new JSlider();
        this.m_PlayerPanel2 = new PlayerPanel(this.m_Model.getPlayer(1));
        this.m_PlayerPanel1 = new PlayerPanel(this.m_Model.getPlayer(0));
        this.m_PlanetScorePanel = new ThreePartScorePanel();
        this.m_NewGameB = new JButton();
        this.m_PauseB = new JToggleButton();
        this.setDefaultCloseOperation(3);
        this.setTitle("Space Wars");
        GroupLayout m_MoreControlsPanelLayout = new GroupLayout(this.m_MoreControlsPanel);
        this.m_MoreControlsPanel.setLayout(m_MoreControlsPanelLayout);
        m_MoreControlsPanelLayout.setHorizontalGroup(m_MoreControlsPanelLayout.createParallelGroup(GroupLayout.Alignment.LEADING).addGap(0, 0, Short.MAX_VALUE));
        m_MoreControlsPanelLayout.setVerticalGroup(m_MoreControlsPanelLayout.createParallelGroup(GroupLayout.Alignment.LEADING).addGap(0, 110, Short.MAX_VALUE));
        this.m_ZoomSlider2.addChangeListener(new ChangeListener(){

            @Override
            public void stateChanged(ChangeEvent evt) {
                FinalDlg.this.m_ZoomSlider2StateChanged(evt);
            }
        });
        this.m_ZoomLabel2.setText("Zoom");
        this.m_ZoomLabel1.setText("Zoom");
        this.m_ZoomSlider1.addChangeListener(new ChangeListener(){

            @Override
            public void stateChanged(ChangeEvent evt) {
                FinalDlg.this.m_ZoomSlider1StateChanged(evt);
            }
        });
        this.m_NewGameB.setText("New Game");
        this.m_NewGameB.addActionListener(new ActionListener(){

            @Override
            public void actionPerformed(ActionEvent evt) {
                FinalDlg.this.m_NewGameBActionPerformed(evt);
            }
        });
        this.m_PauseB.setText("Pause");
        this.m_PauseB.addActionListener(new ActionListener(){

            @Override
            public void actionPerformed(ActionEvent evt) {
                FinalDlg.this.m_PauseBActionPerformed(evt);
            }
        });
        GroupLayout layout = new GroupLayout(this.getContentPane());
        this.getContentPane().setLayout(layout);
        layout.setHorizontalGroup(layout.createParallelGroup(GroupLayout.Alignment.LEADING).addGroup(layout.createSequentialGroup().addGroup(layout.createParallelGroup(GroupLayout.Alignment.TRAILING).addGroup(layout.createSequentialGroup().addComponent((Component)this.m_HeroViewCanvas1, -1, 550, Short.MAX_VALUE).addPreferredGap(LayoutStyle.ComponentPlacement.RELATED).addComponent((Component)this.m_HeroViewCanvas2, -1, 558, Short.MAX_VALUE)).addGroup(layout.createSequentialGroup().addComponent((Component)this.m_WorldViewCanvas1, -1, 385, Short.MAX_VALUE).addPreferredGap(LayoutStyle.ComponentPlacement.RELATED).addGroup(layout.createParallelGroup(GroupLayout.Alignment.LEADING).addComponent(this.m_PlanetScorePanel, -1, 316, Short.MAX_VALUE).addGroup(GroupLayout.Alignment.TRAILING, layout.createSequentialGroup().addComponent(this.m_NewGameB, -2, 156, -2).addPreferredGap(LayoutStyle.ComponentPlacement.RELATED).addComponent(this.m_PauseB, -1, 153, Short.MAX_VALUE)).addComponent(this.m_ZoomLabel2, -2, 40, -2).addComponent(this.m_ZoomLabel1).addComponent(this.m_PlayerPanel2, -1, 316, Short.MAX_VALUE).addGroup(layout.createSequentialGroup().addGap(40, 40, 40).addComponent(this.m_ZoomSlider2, -1, 276, Short.MAX_VALUE)).addComponent(this.m_PlayerPanel1, -1, 316, Short.MAX_VALUE).addGroup(layout.createSequentialGroup().addGap(39, 39, 39).addComponent(this.m_ZoomSlider1, -1, 277, Short.MAX_VALUE))).addPreferredGap(LayoutStyle.ComponentPlacement.RELATED).addComponent((Component)this.m_WorldViewCanvas2, -1, 397, Short.MAX_VALUE))).addPreferredGap(LayoutStyle.ComponentPlacement.RELATED).addComponent(this.m_MoreControlsPanel, -1, -1, Short.MAX_VALUE).addContainerGap()));
        layout.setVerticalGroup(layout.createParallelGroup(GroupLayout.Alignment.LEADING).addGroup(GroupLayout.Alignment.TRAILING, layout.createSequentialGroup().addGroup(layout.createParallelGroup(GroupLayout.Alignment.LEADING).addComponent((Component)this.m_HeroViewCanvas2, -1, 387, Short.MAX_VALUE).addComponent((Component)this.m_HeroViewCanvas1, GroupLayout.Alignment.TRAILING, -1, 387, Short.MAX_VALUE)).addGap(10, 10, 10).addGroup(layout.createParallelGroup(GroupLayout.Alignment.LEADING).addComponent((Component)this.m_WorldViewCanvas2, -1, 278, Short.MAX_VALUE).addGroup(layout.createSequentialGroup().addPreferredGap(LayoutStyle.ComponentPlacement.RELATED, 168, Short.MAX_VALUE).addComponent(this.m_MoreControlsPanel, -1, -1, Short.MAX_VALUE)).addGroup(layout.createSequentialGroup().addPreferredGap(LayoutStyle.ComponentPlacement.RELATED).addGroup(layout.createParallelGroup(GroupLayout.Alignment.LEADING).addComponent((Component)this.m_WorldViewCanvas1, -1, 278, Short.MAX_VALUE).addGroup(GroupLayout.Alignment.TRAILING, layout.createSequentialGroup().addGroup(layout.createParallelGroup(GroupLayout.Alignment.LEADING).addComponent(this.m_ZoomLabel1).addComponent(this.m_ZoomSlider1, -2, -1, -2)).addPreferredGap(LayoutStyle.ComponentPlacement.RELATED).addComponent(this.m_PlayerPanel1, -2, 57, -2).addPreferredGap(LayoutStyle.ComponentPlacement.RELATED).addGroup(layout.createParallelGroup(GroupLayout.Alignment.LEADING).addComponent(this.m_ZoomLabel2).addComponent(this.m_ZoomSlider2, -2, -1, -2)).addPreferredGap(LayoutStyle.ComponentPlacement.RELATED).addComponent(this.m_PlayerPanel2, -2, 57, -2).addPreferredGap(LayoutStyle.ComponentPlacement.RELATED).addComponent(this.m_PlanetScorePanel, -1, 58, Short.MAX_VALUE).addPreferredGap(LayoutStyle.ComponentPlacement.RELATED).addGroup(layout.createParallelGroup(GroupLayout.Alignment.BASELINE).addComponent(this.m_NewGameB).addComponent(this.m_PauseB))))))));
        this.pack();
    }

    private void m_PauseBActionPerformed(ActionEvent evt) {
        this.m_Model.setPause(this.m_PauseB.isSelected());
    }

    private void m_ZoomSlider1StateChanged(ChangeEvent evt) {
        double nextValue = (double)((float)this.m_ZoomSlider1.getValue() / 100.0f) * this.m_Model.getWorldViewHypot();
        nextValue = Math.max(nextValue, 15.0);
        this.m_Model.getViewBounds(0).setSize((float)nextValue);
        this.m_MoreControlsPanel.requestFocus();
    }

    private void m_ZoomSlider2StateChanged(ChangeEvent evt) {
        double nextValue = (double)((float)this.m_ZoomSlider2.getValue() / 100.0f) * this.m_Model.getWorldViewHypot();
        nextValue = Math.max(nextValue, 15.0);
        this.m_Model.getViewBounds(1).setSize((float)nextValue);
        this.m_MoreControlsPanel.requestFocus();
    }

    private void m_NewGameBActionPerformed(ActionEvent evt) {
        this.m_Model.setPause(true);
        this.m_PauseB.setSelected(true);
        GameCreator creator = new GameCreator(this);
        this.setNewModel(creator.getNewModel());
        this.m_Model.setPause(false);
        this.m_PauseB.setSelected(false);
        this.m_MoreControlsPanel.requestFocus();
    }

    private void zoomInPlayer1() {
        int cur_val = this.m_ZoomSlider1.getValue();
        this.m_ZoomSlider1.setValue(cur_val - 1);
        this.m_ZoomSlider1StateChanged(null);
    }

    private void zoomInPlayer2() {
        int cur_val = this.m_ZoomSlider2.getValue();
        this.m_ZoomSlider2.setValue(cur_val - 1);
        this.m_ZoomSlider2StateChanged(null);
    }

    private void zoomOutPlayer1() {
        int cur_val = this.m_ZoomSlider1.getValue();
        this.m_ZoomSlider1.setValue(cur_val + 1);
        this.m_ZoomSlider1StateChanged(null);
    }

    private void zoomOutPlayer2() {
        int cur_val = this.m_ZoomSlider2.getValue();
        this.m_ZoomSlider2.setValue(cur_val + 1);
        this.m_ZoomSlider2StateChanged(null);
    }

    private void handleKeys() {
        if (keyboard.keyDown(85)) {
            this.zoomInPlayer1();
        } else if (keyboard.keyDown(73)) {
            this.zoomOutPlayer1();
        }
        if (keyboard.keyDown(155)) {
            this.zoomInPlayer2();
        } else if (keyboard.keyDown(36)) {
            this.zoomOutPlayer2();
        }
        Ship hero = this.m_Model.getHero(0);
        if (keyboard.keyDown(74)) {
            hero.closeWings();
        } else if (keyboard.wasReleased(74)) {
            hero.openWings();
        } else if (keyboard.keyDown(87)) {
            hero.thrust();
        } else if (keyboard.keyDown(83)) {
            hero.reverse();
        } else if (keyboard.wasReleased(87) || keyboard.wasReleased(83)) {
            hero.thrustHalt();
        }
        if (keyboard.keyDown(65)) {
            hero.turnLeft();
        } else if (keyboard.keyDown(68)) {
            hero.turnRight();
        } else if (keyboard.wasReleased(65) || keyboard.wasReleased(68)) {
            hero.turnHalt();
        }
        if (keyboard.keyDown(32)) {
            hero.fireLaser();
        } else if (keyboard.wasReleased(32)) {
            hero.fireLaserHalt();
        }
        if (keyboard.keyDown(75)) {
            hero.fireCannon();
        } else {
            hero.fireCannonHalt();
        }
        hero = this.m_Model.getHero(1);
        if (keyboard.keyDown(34)) {
            hero.closeWings();
        } else if (keyboard.wasReleased(34)) {
            hero.openWings();
        } else if (keyboard.keyDown(104)) {
            hero.thrust();
        } else if (keyboard.keyDown(101)) {
            hero.reverse();
        } else if (keyboard.wasReleased(104) || keyboard.wasReleased(101)) {
            hero.thrustHalt();
        }
        if (keyboard.keyDown(100)) {
            hero.turnLeft();
        } else if (keyboard.keyDown(102)) {
            hero.turnRight();
        } else if (keyboard.wasReleased(102) || keyboard.wasReleased(100)) {
            hero.turnHalt();
        }
        if (keyboard.keyDown(127)) {
            hero.fireLaser();
        } else if (keyboard.wasReleased(127)) {
            hero.fireLaserHalt();
        }
        if (keyboard.keyDown(35)) {
            hero.fireCannon();
        } else {
            hero.fireCannonHalt();
        }
    }

    @Override
    public void display(GLAutoDrawable drawable) {
        this.m_Model.display(drawable);
    }

    public static void main(String[] args) {
        EventQueue.invokeLater(new Runnable(){

            @Override
            public void run() {
                FinalDlg dlg = new FinalDlg(new Model(GameConfig.STARTUP_CONFIG), true);
                dlg.setVisible(true);
            }
        });
    }

    private static void setGUILook() {
        try {
            UIManager.setLookAndFeel("com.sun.java.swing.plaf.windows.WindowsLookAndFeel");
        } catch (UnsupportedLookAndFeelException e) {
            try {
                UIManager.setLookAndFeel(UIManager.getCrossPlatformLookAndFeelClassName());
            } catch (ClassNotFoundException e1) {
                e1.printStackTrace();
            } catch (InstantiationException e1) {
                e1.printStackTrace();
            } catch (IllegalAccessException e1) {
                e1.printStackTrace();
            } catch (UnsupportedLookAndFeelException e1) {
                e1.printStackTrace();
            }
        } catch (ClassNotFoundException e) {
            System.err.println("! Class Not Found Exception!");
        } catch (InstantiationException e) {
            System.err.println("! Instantiation Exception!");
        } catch (IllegalAccessException e) {
            System.err.println("! Illegal Access Exception!");
        }
        if (UIManager.getLookAndFeel().equals("Metal")) {
            MetalLookAndFeel.setCurrentTheme(new OceanTheme());
        }
    }

    private void setNewModel(Model newModel) {
        if (newModel == null) {
            return;
        }
        this.m_Model.setPause(true);
        this.timey.stop();
        this.m_Model = newModel;
        this.m_WorldCanvasScalar1.setViewBounds(this.m_Model.getWorldViewBounds());
        this.m_WorldCanvasScalar2.setViewBounds(this.m_Model.getWorldViewBounds());
        double sliderValue = this.m_Model.getViewBounds(0).getSize();
        sliderValue = sliderValue / this.m_Model.getWorldViewHypot() * 800.0;
        double nextValue = sliderValue / 100.0 * this.m_Model.getWorldViewHypot();
        this.m_Model.getViewBounds(0).setSize((float)nextValue);
        this.m_ZoomSlider1.setValue((int)sliderValue);
        sliderValue = this.m_Model.getViewBounds(1).getSize();
        sliderValue = sliderValue / this.m_Model.getWorldViewHypot() * 800.0;
        nextValue = sliderValue / 100.0 * this.m_Model.getWorldViewHypot();
        this.m_Model.getViewBounds(1).setSize((float)nextValue);
        this.m_ZoomSlider2.setValue((int)sliderValue);
        ((PlayerPanel)this.m_PlayerPanel1).setPlayer(this.m_Model.getPlayer(0));
        ((PlayerPanel)this.m_PlayerPanel2).setPlayer(this.m_Model.getPlayer(1));
        ThreePartScorePanel planetScore = (ThreePartScorePanel)this.m_PlanetScorePanel;
        planetScore.setLeftColor(this.m_Model.getPlayer(0).getColor().getJavaColor());
        planetScore.setRightColor(this.m_Model.getPlayer(1).getColor().getJavaColor());
        this.timey = new UWBGL_Timer((int)(this.m_Model.getDeltaTime() * 1000.0f), this);
        this.m_MoreControlsPanel.requestFocus();
    }

    private void updatePlanetScore() {
        ThreePartScorePanel scoreBoard = (ThreePartScorePanel)this.m_PlanetScorePanel;
        int left = this.m_Model.getPlayer(0).getPlanetCount();
        int right = this.m_Model.getPlayer(1).getPlanetCount();
        int center = this.m_Model.getPlanetCount() - left - right;
        scoreBoard.setValues(left, center, right);
    }
}

