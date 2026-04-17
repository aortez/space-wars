/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 */
package UWBGL_JavaOpenGL;

import UWBGL_JavaOpenGL.UWBGL_ButtonRepeater;
import UWBGL_JavaOpenGL.UWBGL_Common;
import UWBGL_JavaOpenGL.UWBGL_Dial;
import UWBGL_JavaOpenGL.UWBGL_SceneNode;
import UWBGL_JavaOpenGL.UWBGL_SceneNodeControlListener;
import UWBGL_JavaOpenGL.UWBGL_SceneTree;
import UWBGL_JavaOpenGL.UWBGL_Xform;
import UWBGL_JavaOpenGL.Util.UWBGL_Util;
import java.awt.Color;
import java.awt.Component;
import java.awt.Dimension;
import java.awt.Insets;
import java.awt.event.ActionEvent;
import java.awt.event.ActionListener;
import java.awt.event.MouseAdapter;
import java.awt.event.MouseEvent;
import java.awt.event.MouseMotionAdapter;
import java.util.ArrayList;
import java.util.List;
import javax.swing.BorderFactory;
import javax.swing.GroupLayout;
import javax.swing.JButton;
import javax.swing.JCheckBox;
import javax.swing.JLabel;
import javax.swing.JPanel;
import javax.swing.JScrollPane;
import javax.swing.JTextField;
import javax.swing.JTree;
import javax.swing.LayoutStyle;
import javax.swing.event.TreeSelectionEvent;
import javax.swing.event.TreeSelectionListener;
import org.netbeans.lib.awtextra.AbsoluteConstraints;
import org.netbeans.lib.awtextra.AbsoluteLayout;

public class UWBGL_SceneNodeControl
extends JPanel
implements TreeSelectionListener {
    private UWBGL_SceneNode m_CurrentNode;
    private List<UWBGL_SceneNodeControlListener> m_ControlListeners;
    private ControlType m_CurrentlyDisplaying;
    private UWBGL_ButtonRepeater m_ButtonRepeater;
    private JScrollPane jScrollPane1;
    private JPanel m_ControlPanel;
    private JLabel m_PivotLabel;
    private JButton m_PivotXLeftB;
    private JButton m_PivotXRightB;
    private JButton m_PivotYDownB;
    private JButton m_PivotYUpB;
    private JLabel m_RotateLabel;
    private JLabel m_ScaleLabel;
    private JButton m_ScaleXLeftB;
    private JButton m_ScaleXRightB;
    private JButton m_ScaleYDownB;
    private JButton m_ScaleYUpB;
    private JTree m_SceneTree;
    private JCheckBox m_ShowPivotC;
    private JButton m_TransXLeftB;
    private JButton m_TransXRightB;
    private JButton m_TransYDownB;
    private JButton m_TransYUpB;
    private JLabel m_TranslateLabel;
    private JTextField m_XText;
    private JTextField m_YText;
    private UWBGL_Dial m_RotDial;

    public UWBGL_SceneNodeControl() {
        this.initComponents();
        this.m_CurrentNode = null;
        this.m_CurrentlyDisplaying = ControlType.Trans;
        this.m_SceneTree.addTreeSelectionListener(this);
        this.m_ControlListeners = new ArrayList<UWBGL_SceneNodeControlListener>();
        this.m_ButtonRepeater = new UWBGL_ButtonRepeater();
        this.m_RotDial.setMinimum(0);
        this.m_RotDial.setMaximum(360);
        this.m_RotDial.addMouseListener(new MouseAdapter(){

            @Override
            public void mousePressed(MouseEvent evt) {
                UWBGL_SceneNodeControl.this.m_RotDialChanged(evt);
            }
        });
        this.m_RotDial.addMouseMotionListener(new MouseMotionAdapter(){

            @Override
            public void mouseDragged(MouseEvent evt) {
                UWBGL_SceneNodeControl.this.m_RotDialChanged(evt);
            }
        });
    }

    public UWBGL_SceneNodeControl(UWBGL_SceneNode root) {
        this();
        this.setRoot(root);
    }

    public void setRoot(UWBGL_SceneNode root) {
        UWBGL_SceneTree sceneTree = (UWBGL_SceneTree)this.m_SceneTree;
        sceneTree.setRoot(root);
        this.m_CurrentNode = root;
        this.updateDisplay();
    }

    @Override
    public void valueChanged(TreeSelectionEvent e) {
        this.m_CurrentNode = (UWBGL_SceneNode)e.getPath().getLastPathComponent();
        this.updateDisplay();
    }

    public void updateDisplay() {
        if (this.m_CurrentNode == null) {
            return;
        }
        UWBGL_Xform currentXform = this.m_CurrentNode.getXform();
        this.m_ShowPivotC.setSelected(this.m_CurrentNode.isPivotVisible());
        this.m_TranslateLabel.setText("<HTML>Translate</HTML>");
        this.m_ScaleLabel.setText("<HTML>Scale</HTML>");
        this.m_PivotLabel.setText("<HTML>Pivot</HTML>");
        this.m_RotateLabel.setText("<HTML>Rotate</HTML>");
        switch (this.m_CurrentlyDisplaying) {
            case Trans: {
                this.m_XText.setText(UWBGL_Util.truncateStr("" + currentXform.getTranslation().x, 1));
                this.m_YText.setText(UWBGL_Util.truncateStr("" + currentXform.getTranslation().y, 1));
                this.m_TranslateLabel.setText("<HTML><U>Translate</U></HTML>");
                break;
            }
            case Scale: {
                this.m_XText.setText(UWBGL_Util.truncateStr("" + currentXform.getScale().x, 1));
                this.m_YText.setText(UWBGL_Util.truncateStr("" + currentXform.getScale().y, 1));
                this.m_ScaleLabel.setText("<HTML><U>Scale</U></HTML>");
                break;
            }
            case Pivot: {
                this.m_XText.setText(UWBGL_Util.truncateStr("" + currentXform.getPivot().x, 1));
                this.m_YText.setText(UWBGL_Util.truncateStr("" + currentXform.getPivot().y, 1));
                this.m_PivotLabel.setText("<HTML><U>Pivot</U></HTML>");
                break;
            }
            case Rotate: {
                this.m_XText.setText(UWBGL_Util.truncateStr("" + currentXform.getRotationInDegrees(), 0));
                this.m_YText.setText("");
                this.m_RotDial.setValue((int)currentXform.getRotationInDegrees());
                this.m_RotDial.repaint();
                this.m_RotateLabel.setText("<HTML><U>Rotate</U></HTML>");
            }
        }
    }

    public void addSceneNodeControlListener(UWBGL_SceneNodeControlListener l) {
        this.m_ControlListeners.add(l);
    }

    public void removeSceneNodeControlListener(UWBGL_SceneNodeControlListener l) {
        this.m_ControlListeners.remove(l);
    }

    public UWBGL_SceneNode getCurrentNode() {
        return this.m_CurrentNode;
    }

    public void addTreeSelectionListener(TreeSelectionListener l) {
        this.m_SceneTree.addTreeSelectionListener(l);
    }

    public void removeTreeSelectionListener(TreeSelectionListener l) {
        this.m_SceneTree.removeTreeSelectionListener(l);
    }

    private void initComponents() {
        this.m_ControlPanel = new JPanel();
        this.m_TransYUpB = new JButton();
        this.m_PivotYUpB = new JButton();
        this.m_ScaleYUpB = new JButton();
        this.m_TransYDownB = new JButton();
        this.m_PivotYDownB = new JButton();
        this.m_ScaleYDownB = new JButton();
        this.m_TransXLeftB = new JButton();
        this.m_ScaleXLeftB = new JButton();
        this.m_PivotXLeftB = new JButton();
        this.m_TransXRightB = new JButton();
        this.m_ScaleXRightB = new JButton();
        this.m_PivotXRightB = new JButton();
        this.m_YText = new JTextField();
        this.m_XText = new JTextField();
        this.jScrollPane1 = new JScrollPane();
        this.m_SceneTree = new UWBGL_SceneTree();
        this.m_TranslateLabel = new JLabel();
        this.m_ScaleLabel = new JLabel();
        this.m_PivotLabel = new JLabel();
        this.m_RotateLabel = new JLabel();
        this.m_ShowPivotC = new JCheckBox();
        this.m_ControlPanel.setBorder(BorderFactory.createBevelBorder(0));
        this.m_ControlPanel.setLayout(new AbsoluteLayout());
        this.m_TransYUpB.setForeground(new Color(255, 0, 0));
        this.m_TransYUpB.setText("^");
        this.m_TransYUpB.setToolTipText("Translate");
        this.m_TransYUpB.setMargin(new Insets(2, 0, 2, 0));
        this.m_TransYUpB.setPreferredSize(new Dimension(41, 15));
        this.m_TransYUpB.addMouseListener(new MouseAdapter(){

            @Override
            public void mousePressed(MouseEvent evt) {
                UWBGL_SceneNodeControl.this.m_TransYUpBMousePressed(evt);
            }

            @Override
            public void mouseReleased(MouseEvent evt) {
                UWBGL_SceneNodeControl.this.m_TransYUpBMouseReleased(evt);
            }
        });
        this.m_TransYUpB.addActionListener(new ActionListener(){

            @Override
            public void actionPerformed(ActionEvent evt) {
                UWBGL_SceneNodeControl.this.m_TransYUpBActionPerformed(evt);
            }
        });
        this.m_ControlPanel.add((Component)this.m_TransYUpB, new AbsoluteConstraints(70, 10, 40, 20));
        this.m_PivotYUpB.setForeground(new Color(0, 204, 0));
        this.m_PivotYUpB.setText("^");
        this.m_PivotYUpB.setToolTipText("Pivot");
        this.m_PivotYUpB.setMargin(new Insets(2, 0, 2, 0));
        this.m_PivotYUpB.setMaximumSize(new Dimension(13, 20));
        this.m_PivotYUpB.setMinimumSize(new Dimension(13, 20));
        this.m_PivotYUpB.setPreferredSize(new Dimension(41, 20));
        this.m_PivotYUpB.addMouseListener(new MouseAdapter(){

            @Override
            public void mousePressed(MouseEvent evt) {
                UWBGL_SceneNodeControl.this.m_PivotYUpBMousePressed(evt);
            }

            @Override
            public void mouseReleased(MouseEvent evt) {
                UWBGL_SceneNodeControl.this.m_PivotYUpBMouseReleased(evt);
            }
        });
        this.m_PivotYUpB.addActionListener(new ActionListener(){

            @Override
            public void actionPerformed(ActionEvent evt) {
                UWBGL_SceneNodeControl.this.m_PivotYUpBActionPerformed(evt);
            }
        });
        this.m_ControlPanel.add((Component)this.m_PivotYUpB, new AbsoluteConstraints(70, 50, 40, 20));
        this.m_ScaleYUpB.setForeground(new Color(204, 204, 0));
        this.m_ScaleYUpB.setText("^");
        this.m_ScaleYUpB.setToolTipText("Scale");
        this.m_ScaleYUpB.setMargin(new Insets(2, 0, 2, 0));
        this.m_ScaleYUpB.setPreferredSize(new Dimension(41, 15));
        this.m_ScaleYUpB.addMouseListener(new MouseAdapter(){

            @Override
            public void mousePressed(MouseEvent evt) {
                UWBGL_SceneNodeControl.this.m_ScaleYUpBMousePressed(evt);
            }

            @Override
            public void mouseReleased(MouseEvent evt) {
                UWBGL_SceneNodeControl.this.m_ScaleYUpBMouseReleased(evt);
            }
        });
        this.m_ScaleYUpB.addActionListener(new ActionListener(){

            @Override
            public void actionPerformed(ActionEvent evt) {
                UWBGL_SceneNodeControl.this.m_ScaleYUpBActionPerformed(evt);
            }
        });
        this.m_ControlPanel.add((Component)this.m_ScaleYUpB, new AbsoluteConstraints(70, 30, 40, 20));
        this.m_TransYDownB.setForeground(new Color(255, 0, 0));
        this.m_TransYDownB.setText("\u02c5");
        this.m_TransYDownB.setToolTipText("Translate");
        this.m_TransYDownB.setMargin(new Insets(2, 0, 2, 0));
        this.m_TransYDownB.setPreferredSize(new Dimension(41, 15));
        this.m_TransYDownB.addMouseListener(new MouseAdapter(){

            @Override
            public void mousePressed(MouseEvent evt) {
                UWBGL_SceneNodeControl.this.m_TransYDownBMousePressed(evt);
            }

            @Override
            public void mouseReleased(MouseEvent evt) {
                UWBGL_SceneNodeControl.this.m_TransYDownBMouseReleased(evt);
            }
        });
        this.m_TransYDownB.addActionListener(new ActionListener(){

            @Override
            public void actionPerformed(ActionEvent evt) {
                UWBGL_SceneNodeControl.this.m_TransYDownBActionPerformed(evt);
            }
        });
        this.m_ControlPanel.add((Component)this.m_TransYDownB, new AbsoluteConstraints(70, 150, 40, 20));
        this.m_PivotYDownB.setForeground(new Color(0, 204, 0));
        this.m_PivotYDownB.setText("\u02c5");
        this.m_PivotYDownB.setToolTipText("Pivot");
        this.m_PivotYDownB.setMargin(new Insets(2, 0, 2, 0));
        this.m_PivotYDownB.setPreferredSize(new Dimension(41, 15));
        this.m_PivotYDownB.addMouseListener(new MouseAdapter(){

            @Override
            public void mousePressed(MouseEvent evt) {
                UWBGL_SceneNodeControl.this.m_PivotYDownBMousePressed(evt);
            }

            @Override
            public void mouseReleased(MouseEvent evt) {
                UWBGL_SceneNodeControl.this.m_PivotYDownBMouseReleased(evt);
            }
        });
        this.m_PivotYDownB.addActionListener(new ActionListener(){

            @Override
            public void actionPerformed(ActionEvent evt) {
                UWBGL_SceneNodeControl.this.m_PivotYDownBActionPerformed(evt);
            }
        });
        this.m_ControlPanel.add((Component)this.m_PivotYDownB, new AbsoluteConstraints(70, 110, 40, 20));
        this.m_ScaleYDownB.setForeground(new Color(204, 204, 0));
        this.m_ScaleYDownB.setText("\u02c5");
        this.m_ScaleYDownB.setToolTipText("Scale");
        this.m_ScaleYDownB.setMargin(new Insets(2, 0, 2, 0));
        this.m_ScaleYDownB.setPreferredSize(new Dimension(41, 15));
        this.m_ScaleYDownB.addMouseListener(new MouseAdapter(){

            @Override
            public void mousePressed(MouseEvent evt) {
                UWBGL_SceneNodeControl.this.m_ScaleYDownBMousePressed(evt);
            }

            @Override
            public void mouseReleased(MouseEvent evt) {
                UWBGL_SceneNodeControl.this.m_ScaleYDownBMouseReleased(evt);
            }
        });
        this.m_ScaleYDownB.addActionListener(new ActionListener(){

            @Override
            public void actionPerformed(ActionEvent evt) {
                UWBGL_SceneNodeControl.this.m_ScaleYDownBActionPerformed(evt);
            }
        });
        this.m_ControlPanel.add((Component)this.m_ScaleYDownB, new AbsoluteConstraints(70, 130, 40, 20));
        this.m_TransXLeftB.setForeground(new Color(255, 0, 0));
        this.m_TransXLeftB.setText("<");
        this.m_TransXLeftB.setToolTipText("Translate");
        this.m_TransXLeftB.setMargin(new Insets(2, 0, 2, 0));
        this.m_TransXLeftB.setPreferredSize(new Dimension(41, 15));
        this.m_TransXLeftB.addMouseListener(new MouseAdapter(){

            @Override
            public void mousePressed(MouseEvent evt) {
                UWBGL_SceneNodeControl.this.m_TransXLeftBMousePressed(evt);
            }

            @Override
            public void mouseReleased(MouseEvent evt) {
                UWBGL_SceneNodeControl.this.m_TransXLeftBMouseReleased(evt);
            }
        });
        this.m_TransXLeftB.addActionListener(new ActionListener(){

            @Override
            public void actionPerformed(ActionEvent evt) {
                UWBGL_SceneNodeControl.this.m_TransXLeftBActionPerformed(evt);
            }
        });
        this.m_ControlPanel.add((Component)this.m_TransXLeftB, new AbsoluteConstraints(10, 70, 20, 40));
        this.m_ScaleXLeftB.setForeground(new Color(204, 204, 0));
        this.m_ScaleXLeftB.setText("<");
        this.m_ScaleXLeftB.setToolTipText("Scale");
        this.m_ScaleXLeftB.setMargin(new Insets(2, 0, 2, 0));
        this.m_ScaleXLeftB.setPreferredSize(new Dimension(41, 15));
        this.m_ScaleXLeftB.addMouseListener(new MouseAdapter(){

            @Override
            public void mousePressed(MouseEvent evt) {
                UWBGL_SceneNodeControl.this.m_ScaleXLeftBMousePressed(evt);
            }

            @Override
            public void mouseReleased(MouseEvent evt) {
                UWBGL_SceneNodeControl.this.m_ScaleXLeftBMouseReleased(evt);
            }
        });
        this.m_ScaleXLeftB.addActionListener(new ActionListener(){

            @Override
            public void actionPerformed(ActionEvent evt) {
                UWBGL_SceneNodeControl.this.m_ScaleXLeftBActionPerformed(evt);
            }
        });
        this.m_ControlPanel.add((Component)this.m_ScaleXLeftB, new AbsoluteConstraints(30, 70, 20, 40));
        this.m_PivotXLeftB.setForeground(new Color(0, 204, 0));
        this.m_PivotXLeftB.setText("<");
        this.m_PivotXLeftB.setToolTipText("Pivot");
        this.m_PivotXLeftB.setMargin(new Insets(2, 0, 2, 0));
        this.m_PivotXLeftB.setPreferredSize(new Dimension(41, 15));
        this.m_PivotXLeftB.addMouseListener(new MouseAdapter(){

            @Override
            public void mousePressed(MouseEvent evt) {
                UWBGL_SceneNodeControl.this.m_PivotXLeftBMousePressed(evt);
            }

            @Override
            public void mouseReleased(MouseEvent evt) {
                UWBGL_SceneNodeControl.this.m_PivotXLeftBMouseReleased(evt);
            }
        });
        this.m_PivotXLeftB.addActionListener(new ActionListener(){

            @Override
            public void actionPerformed(ActionEvent evt) {
                UWBGL_SceneNodeControl.this.m_PivotXLeftBActionPerformed(evt);
            }
        });
        this.m_ControlPanel.add((Component)this.m_PivotXLeftB, new AbsoluteConstraints(50, 70, 20, 40));
        this.m_TransXRightB.setForeground(new Color(255, 0, 0));
        this.m_TransXRightB.setText(">");
        this.m_TransXRightB.setToolTipText("Translate");
        this.m_TransXRightB.setMargin(new Insets(2, 0, 2, 0));
        this.m_TransXRightB.setPreferredSize(new Dimension(41, 15));
        this.m_TransXRightB.addMouseListener(new MouseAdapter(){

            @Override
            public void mousePressed(MouseEvent evt) {
                UWBGL_SceneNodeControl.this.m_TransXRightBMousePressed(evt);
            }

            @Override
            public void mouseReleased(MouseEvent evt) {
                UWBGL_SceneNodeControl.this.m_TransXRightBMouseReleased(evt);
            }
        });
        this.m_TransXRightB.addActionListener(new ActionListener(){

            @Override
            public void actionPerformed(ActionEvent evt) {
                UWBGL_SceneNodeControl.this.m_TransXRightBActionPerformed(evt);
            }
        });
        this.m_ControlPanel.add((Component)this.m_TransXRightB, new AbsoluteConstraints(150, 70, 20, 40));
        this.m_ScaleXRightB.setForeground(new Color(204, 204, 0));
        this.m_ScaleXRightB.setText(">");
        this.m_ScaleXRightB.setToolTipText("Scale");
        this.m_ScaleXRightB.setMargin(new Insets(2, 0, 2, 0));
        this.m_ScaleXRightB.setPreferredSize(new Dimension(41, 15));
        this.m_ScaleXRightB.addMouseListener(new MouseAdapter(){

            @Override
            public void mousePressed(MouseEvent evt) {
                UWBGL_SceneNodeControl.this.m_ScaleXRightBMousePressed(evt);
            }

            @Override
            public void mouseReleased(MouseEvent evt) {
                UWBGL_SceneNodeControl.this.m_ScaleXRightBMouseReleased(evt);
            }
        });
        this.m_ScaleXRightB.addActionListener(new ActionListener(){

            @Override
            public void actionPerformed(ActionEvent evt) {
                UWBGL_SceneNodeControl.this.m_ScaleXRightBActionPerformed(evt);
            }
        });
        this.m_ControlPanel.add((Component)this.m_ScaleXRightB, new AbsoluteConstraints(130, 70, 20, 40));
        this.m_PivotXRightB.setForeground(new Color(0, 204, 0));
        this.m_PivotXRightB.setText(">");
        this.m_PivotXRightB.setToolTipText("Pivot");
        this.m_PivotXRightB.setMargin(new Insets(2, 0, 2, 0));
        this.m_PivotXRightB.setPreferredSize(new Dimension(41, 15));
        this.m_PivotXRightB.addMouseListener(new MouseAdapter(){

            @Override
            public void mousePressed(MouseEvent evt) {
                UWBGL_SceneNodeControl.this.m_PivotXRightBMousePressed(evt);
            }

            @Override
            public void mouseReleased(MouseEvent evt) {
                UWBGL_SceneNodeControl.this.m_PivotXRightBMouseReleased(evt);
            }
        });
        this.m_PivotXRightB.addActionListener(new ActionListener(){

            @Override
            public void actionPerformed(ActionEvent evt) {
                UWBGL_SceneNodeControl.this.m_PivotXRightBActionPerformed(evt);
            }
        });
        this.m_ControlPanel.add((Component)this.m_PivotXRightB, new AbsoluteConstraints(110, 70, 20, 40));
        this.m_YText.setHorizontalAlignment(0);
        this.m_YText.setText("360");
        this.m_ControlPanel.add((Component)this.m_YText, new AbsoluteConstraints(70, 90, 40, -1));
        this.m_XText.setHorizontalAlignment(0);
        this.m_XText.setText("150");
        this.m_ControlPanel.add((Component)this.m_XText, new AbsoluteConstraints(70, 70, 40, -1));
        this.m_RotDial = new UWBGL_Dial();
        this.m_RotDial.setToolTipText("Rotate");
        this.m_RotDial.setPreferredSize(new Dimension(179, 179));
        this.m_ControlPanel.add((Component)this.m_RotDial, new AbsoluteConstraints(1, 1, -1, -1));
        this.jScrollPane1.setViewportView(this.m_SceneTree);
        this.m_TranslateLabel.setForeground(new Color(255, 0, 0));
        this.m_TranslateLabel.setText("Translate");
        this.m_TranslateLabel.addMouseListener(new MouseAdapter(){

            @Override
            public void mouseEntered(MouseEvent evt) {
                UWBGL_SceneNodeControl.this.m_TranslateLabelMouseEntered(evt);
            }
        });
        this.m_ScaleLabel.setForeground(new Color(204, 204, 0));
        this.m_ScaleLabel.setText("Scale");
        this.m_ScaleLabel.addMouseListener(new MouseAdapter(){

            @Override
            public void mouseEntered(MouseEvent evt) {
                UWBGL_SceneNodeControl.this.m_ScaleLabelMouseEntered(evt);
            }
        });
        this.m_PivotLabel.setForeground(new Color(0, 204, 0));
        this.m_PivotLabel.setText("Pivot");
        this.m_PivotLabel.addMouseListener(new MouseAdapter(){

            @Override
            public void mouseEntered(MouseEvent evt) {
                UWBGL_SceneNodeControl.this.m_PivotLabelMouseEntered(evt);
            }
        });
        this.m_RotateLabel.setText("Rotate");
        this.m_RotateLabel.addMouseListener(new MouseAdapter(){

            @Override
            public void mouseEntered(MouseEvent evt) {
                UWBGL_SceneNodeControl.this.m_RotateLabelMouseEntered(evt);
            }
        });
        this.m_ShowPivotC.setText("Show Pivot");
        this.m_ShowPivotC.addActionListener(new ActionListener(){

            @Override
            public void actionPerformed(ActionEvent evt) {
                UWBGL_SceneNodeControl.this.m_ShowPivotCActionPerformed(evt);
            }
        });
        GroupLayout layout = new GroupLayout(this);
        this.setLayout(layout);
        layout.setHorizontalGroup(layout.createParallelGroup(GroupLayout.Alignment.LEADING).addGroup(layout.createSequentialGroup().addGroup(layout.createParallelGroup(GroupLayout.Alignment.LEADING).addGroup(GroupLayout.Alignment.TRAILING, layout.createSequentialGroup().addContainerGap().addComponent(this.jScrollPane1, -1, 170, Short.MAX_VALUE)).addGroup(GroupLayout.Alignment.TRAILING, layout.createSequentialGroup().addContainerGap(-1, Short.MAX_VALUE).addComponent(this.m_TranslateLabel).addPreferredGap(LayoutStyle.ComponentPlacement.RELATED).addComponent(this.m_ScaleLabel).addPreferredGap(LayoutStyle.ComponentPlacement.RELATED).addComponent(this.m_PivotLabel).addPreferredGap(LayoutStyle.ComponentPlacement.RELATED).addComponent(this.m_RotateLabel)).addComponent(this.m_ControlPanel, -1, 182, Short.MAX_VALUE).addGroup(layout.createSequentialGroup().addContainerGap().addComponent(this.m_ShowPivotC))).addContainerGap()));
        layout.setVerticalGroup(layout.createParallelGroup(GroupLayout.Alignment.LEADING).addGroup(layout.createSequentialGroup().addComponent(this.jScrollPane1, -2, 140, -2).addPreferredGap(LayoutStyle.ComponentPlacement.RELATED, 4, Short.MAX_VALUE).addComponent(this.m_ShowPivotC).addPreferredGap(LayoutStyle.ComponentPlacement.RELATED).addGroup(layout.createParallelGroup(GroupLayout.Alignment.BASELINE).addComponent(this.m_TranslateLabel, -2, 24, -2).addComponent(this.m_ScaleLabel).addComponent(this.m_PivotLabel).addComponent(this.m_RotateLabel)).addPreferredGap(LayoutStyle.ComponentPlacement.RELATED).addComponent(this.m_ControlPanel, -2, 183, -2)));
    }

    private void m_TransYUpBActionPerformed(ActionEvent evt) {
        for (UWBGL_SceneNodeControlListener listener : this.m_ControlListeners) {
            listener.translateNodeY(this.m_CurrentNode, 1);
        }
        this.m_CurrentlyDisplaying = ControlType.Trans;
        this.updateDisplay();
    }

    private void m_ScaleYUpBActionPerformed(ActionEvent evt) {
        for (UWBGL_SceneNodeControlListener listener : this.m_ControlListeners) {
            listener.scaleNodeY(this.m_CurrentNode, 1);
        }
        this.m_CurrentlyDisplaying = ControlType.Scale;
        this.updateDisplay();
    }

    private void m_PivotYUpBActionPerformed(ActionEvent evt) {
        for (UWBGL_SceneNodeControlListener listener : this.m_ControlListeners) {
            listener.pivotNodeY(this.m_CurrentNode, 1);
        }
        this.m_CurrentlyDisplaying = ControlType.Pivot;
        this.updateDisplay();
    }

    private void m_TransXRightBActionPerformed(ActionEvent evt) {
        for (UWBGL_SceneNodeControlListener listener : this.m_ControlListeners) {
            listener.translateNodeX(this.m_CurrentNode, 1);
        }
        this.m_CurrentlyDisplaying = ControlType.Trans;
        this.updateDisplay();
    }

    private void m_ScaleXRightBActionPerformed(ActionEvent evt) {
        for (UWBGL_SceneNodeControlListener listener : this.m_ControlListeners) {
            listener.scaleNodeX(this.m_CurrentNode, 1);
        }
        this.m_CurrentlyDisplaying = ControlType.Scale;
        this.updateDisplay();
    }

    private void m_PivotXRightBActionPerformed(ActionEvent evt) {
        for (UWBGL_SceneNodeControlListener listener : this.m_ControlListeners) {
            listener.pivotNodeX(this.m_CurrentNode, 1);
        }
        this.m_CurrentlyDisplaying = ControlType.Pivot;
        this.updateDisplay();
    }

    private void m_TransYDownBActionPerformed(ActionEvent evt) {
        for (UWBGL_SceneNodeControlListener listener : this.m_ControlListeners) {
            listener.translateNodeY(this.m_CurrentNode, -1);
        }
        this.m_CurrentlyDisplaying = ControlType.Trans;
        this.updateDisplay();
    }

    private void m_ScaleYDownBActionPerformed(ActionEvent evt) {
        for (UWBGL_SceneNodeControlListener listener : this.m_ControlListeners) {
            listener.scaleNodeY(this.m_CurrentNode, -1);
        }
        this.m_CurrentlyDisplaying = ControlType.Scale;
        this.updateDisplay();
    }

    private void m_PivotYDownBActionPerformed(ActionEvent evt) {
        for (UWBGL_SceneNodeControlListener listener : this.m_ControlListeners) {
            listener.pivotNodeY(this.m_CurrentNode, -1);
        }
        this.m_CurrentlyDisplaying = ControlType.Pivot;
        this.updateDisplay();
    }

    private void m_TransXLeftBActionPerformed(ActionEvent evt) {
        for (UWBGL_SceneNodeControlListener listener : this.m_ControlListeners) {
            listener.translateNodeX(this.m_CurrentNode, -1);
        }
        this.m_CurrentlyDisplaying = ControlType.Trans;
        this.updateDisplay();
    }

    private void m_ScaleXLeftBActionPerformed(ActionEvent evt) {
        for (UWBGL_SceneNodeControlListener listener : this.m_ControlListeners) {
            listener.scaleNodeX(this.m_CurrentNode, -1);
        }
        this.m_CurrentlyDisplaying = ControlType.Scale;
        this.updateDisplay();
    }

    private void m_PivotXLeftBActionPerformed(ActionEvent evt) {
        for (UWBGL_SceneNodeControlListener listener : this.m_ControlListeners) {
            listener.pivotNodeX(this.m_CurrentNode, -1);
        }
        this.m_CurrentlyDisplaying = ControlType.Pivot;
        this.updateDisplay();
    }

    private void m_TranslateLabelMouseEntered(MouseEvent evt) {
        this.m_CurrentlyDisplaying = ControlType.Trans;
        this.updateDisplay();
    }

    private void m_ScaleLabelMouseEntered(MouseEvent evt) {
        this.m_CurrentlyDisplaying = ControlType.Scale;
        this.updateDisplay();
    }

    private void m_PivotLabelMouseEntered(MouseEvent evt) {
        this.m_CurrentlyDisplaying = ControlType.Pivot;
        this.updateDisplay();
    }

    private void m_RotateLabelMouseEntered(MouseEvent evt) {
        this.m_CurrentlyDisplaying = ControlType.Rotate;
        this.updateDisplay();
    }

    private void m_TransYUpBMousePressed(MouseEvent evt) {
        this.m_ButtonRepeater.setButton((JButton)evt.getSource());
        this.m_ButtonRepeater.start();
    }

    private void m_TransYUpBMouseReleased(MouseEvent evt) {
        this.m_ButtonRepeater.stop();
    }

    private void m_ScaleYUpBMousePressed(MouseEvent evt) {
        this.m_ButtonRepeater.setButton((JButton)evt.getSource());
        this.m_ButtonRepeater.start();
    }

    private void m_ScaleYUpBMouseReleased(MouseEvent evt) {
        this.m_ButtonRepeater.stop();
    }

    private void m_PivotYUpBMousePressed(MouseEvent evt) {
        this.m_ButtonRepeater.setButton((JButton)evt.getSource());
        this.m_ButtonRepeater.start();
    }

    private void m_PivotYUpBMouseReleased(MouseEvent evt) {
        this.m_ButtonRepeater.stop();
    }

    private void m_TransXRightBMousePressed(MouseEvent evt) {
        this.m_ButtonRepeater.setButton((JButton)evt.getSource());
        this.m_ButtonRepeater.start();
    }

    private void m_TransXRightBMouseReleased(MouseEvent evt) {
        this.m_ButtonRepeater.stop();
    }

    private void m_ScaleXRightBMousePressed(MouseEvent evt) {
        this.m_ButtonRepeater.setButton((JButton)evt.getSource());
        this.m_ButtonRepeater.start();
    }

    private void m_ScaleXRightBMouseReleased(MouseEvent evt) {
        this.m_ButtonRepeater.stop();
    }

    private void m_PivotXRightBMousePressed(MouseEvent evt) {
        this.m_ButtonRepeater.setButton((JButton)evt.getSource());
        this.m_ButtonRepeater.start();
    }

    private void m_PivotXRightBMouseReleased(MouseEvent evt) {
        this.m_ButtonRepeater.stop();
    }

    private void m_TransYDownBMousePressed(MouseEvent evt) {
        this.m_ButtonRepeater.setButton((JButton)evt.getSource());
        this.m_ButtonRepeater.start();
    }

    private void m_TransYDownBMouseReleased(MouseEvent evt) {
        this.m_ButtonRepeater.stop();
    }

    private void m_ScaleYDownBMousePressed(MouseEvent evt) {
        this.m_ButtonRepeater.setButton((JButton)evt.getSource());
        this.m_ButtonRepeater.start();
    }

    private void m_ScaleYDownBMouseReleased(MouseEvent evt) {
        this.m_ButtonRepeater.stop();
    }

    private void m_PivotYDownBMousePressed(MouseEvent evt) {
        this.m_ButtonRepeater.setButton((JButton)evt.getSource());
        this.m_ButtonRepeater.start();
    }

    private void m_PivotYDownBMouseReleased(MouseEvent evt) {
        this.m_ButtonRepeater.stop();
    }

    private void m_TransXLeftBMousePressed(MouseEvent evt) {
        this.m_ButtonRepeater.setButton((JButton)evt.getSource());
        this.m_ButtonRepeater.start();
    }

    private void m_TransXLeftBMouseReleased(MouseEvent evt) {
        this.m_ButtonRepeater.stop();
    }

    private void m_ScaleXLeftBMousePressed(MouseEvent evt) {
        this.m_ButtonRepeater.setButton((JButton)evt.getSource());
        this.m_ButtonRepeater.start();
    }

    private void m_ScaleXLeftBMouseReleased(MouseEvent evt) {
        this.m_ButtonRepeater.stop();
    }

    private void m_PivotXLeftBMousePressed(MouseEvent evt) {
        this.m_ButtonRepeater.setButton((JButton)evt.getSource());
        this.m_ButtonRepeater.start();
    }

    private void m_PivotXLeftBMouseReleased(MouseEvent evt) {
        this.m_ButtonRepeater.stop();
    }

    private void m_ShowPivotCActionPerformed(ActionEvent evt) {
        for (UWBGL_SceneNodeControlListener listener : this.m_ControlListeners) {
            listener.setShowPivot(this.m_CurrentNode, this.m_ShowPivotC.isSelected());
        }
        this.m_CurrentlyDisplaying = ControlType.Pivot;
        this.updateDisplay();
    }

    private void m_RotDialChanged(MouseEvent evt) {
        int degrees = this.m_RotDial.getValue();
        float radians = UWBGL_Common.toRadians(degrees);
        for (UWBGL_SceneNodeControlListener listener : this.m_ControlListeners) {
            listener.setRotationNode(this.m_CurrentNode, radians);
        }
        this.m_CurrentlyDisplaying = ControlType.Rotate;
        this.updateDisplay();
    }

    private static enum ControlType {
        Trans,
        Scale,
        Pivot,
        Rotate;

    }
}

