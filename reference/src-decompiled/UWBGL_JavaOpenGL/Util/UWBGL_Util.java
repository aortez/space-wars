/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 */
package UWBGL_JavaOpenGL.Util;

import UWBGL_JavaOpenGL.math3d.Vec3;
import java.util.Random;

public class UWBGL_Util {
    private static Random randomizer = new Random();

    public static float randomNumber(float range) {
        return range * randomizer.nextFloat();
    }

    public static float randomNumber(float minRange, float maxRange) {
        float size = maxRange - minRange;
        return minRange + UWBGL_Util.randomNumber(size);
    }

    public static float area(Vec3 p1, Vec3 p2, Vec3 p3) {
        float length1to2 = p1.distanceTo(p2);
        float length2to3 = p2.distanceTo(p3);
        float length3to1 = p3.distanceTo(p1);
        float s = (length1to2 + length2to3 + length3to1) / 2.0f;
        float intermediate = s * (s - length1to2) * (s - length2to3) * (s - length3to1);
        return (float)Math.sqrt(intermediate);
    }

    public static String padStr(String s, int size) {
        while (s.length() < size) {
            s = s + " ";
        }
        return s;
    }

    public static String truncateStr(String s, int precision) {
        int dot_index = s.lastIndexOf(".");
        if (dot_index > 0 && precision > 0) {
            while (dot_index + precision + 1 < s.length()) {
                s = s.substring(0, s.length() - 1);
            }
        } else if (dot_index > 0 && precision == 0) {
            s = s.substring(0, dot_index);
        }
        return s;
    }
}

