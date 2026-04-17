/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 */
import UWBGL_JavaOpenGL.UWBGL_Color;
import java.util.LinkedList;
import java.util.List;

public class Player {
    private int m_Id = 0;
    private String m_Name = "";
    private Ship m_Ship;
    private UWBGL_Color m_Color;
    private List<Planet> m_Planets = new LinkedList<Planet>();

    public Player(int id, String name, Ship ship, UWBGL_Color color) {
        this.m_Id = id;
        this.m_Name = name;
        this.m_Ship = ship;
        this.m_Color = color;
    }

    public void setShip(Ship s) {
        this.m_Ship = s;
    }

    public String getName() {
        return this.m_Name;
    }

    public int getId() {
        return this.m_Id;
    }

    public Ship getShip() {
        return this.m_Ship;
    }

    public UWBGL_Color getColor() {
        return this.m_Color;
    }

    public boolean ownsPlanet(Planet p) {
        return this.m_Planets.contains(p);
    }

    public void claimPlanet(Planet p) {
        if (!this.ownsPlanet(p)) {
            this.m_Planets.add(p);
        }
    }

    public void losePlanet(Planet p) {
        this.m_Planets.remove(p);
    }

    public int getPlanetCount() {
        return this.m_Planets.size();
    }
}

