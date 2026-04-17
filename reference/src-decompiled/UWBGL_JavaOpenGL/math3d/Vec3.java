/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 */
package UWBGL_JavaOpenGL.math3d;

import UWBGL_JavaOpenGL.math3d.Mat4;

public class Vec3 {
    public float x;
    public float y;
    public float z;

    public Vec3() {
        this.z = 0.0f;
        this.y = 0.0f;
        this.x = 0.0f;
    }

    public Vec3(float x) {
        this.x = x;
        this.z = 0.0f;
        this.y = 0.0f;
    }

    public Vec3(float x, float y) {
        this.x = x;
        this.y = y;
        this.z = 0.0f;
    }

    public Vec3(float x, float y, float z) {
        this.x = x;
        this.y = y;
        this.z = z;
    }

    public Vec3(Vec3 other) {
        this.x = other.x;
        this.y = other.y;
        this.z = other.z;
    }

    public void set(Vec3 other) {
        this.x = other.x;
        this.y = other.y;
        this.z = other.z;
    }

    public Vec3 set(float x, float y) {
        this.x = x;
        this.y = y;
        return this;
    }

    public Vec3 set(float x, float y, float z) {
        this.x = x;
        this.y = y;
        this.z = z;
        return this;
    }

    public Vec3 setPlus(float x, float y, float z) {
        this.x += x;
        this.y += y;
        this.z += z;
        return this;
    }

    public Vec3 setPlus(float x, float y) {
        this.x += x;
        this.y += y;
        return this;
    }

    public void setX(float x) {
        this.x = x;
    }

    public void setY(float y) {
        this.y = y;
    }

    public void setZ(float z) {
        this.z = z;
    }

    public Vec3 times(float mult) {
        return this.clone().timesEquals(mult);
    }

    public Vec3 timesEquals(float mult) {
        this.x *= mult;
        this.y *= mult;
        return this;
    }

    public Vec3 times(Vec3 mult) {
        return this.clone().timesEquals(mult);
    }

    public Vec3 timesEquals(Vec3 mult) {
        this.x *= mult.x;
        this.y *= mult.y;
        this.z = mult.z;
        return this;
    }

    public Vec3 divide(float mult) {
        return this.clone().divideEquals(mult);
    }

    public Vec3 divideEquals(float mult) {
        if (mult != 0.0f) {
            this.x /= mult;
            this.y /= mult;
        }
        return this;
    }

    public Vec3 plus(Vec3 other) {
        return this.clone().plusEquals(other);
    }

    public Vec3 plusEquals(Vec3 other) {
        this.x += other.x;
        this.y += other.y;
        this.z = other.z;
        return this;
    }

    public Vec3 minus(Vec3 other) {
        return this.clone().minusEquals(other);
    }

    public Vec3 minusEquals(Vec3 other) {
        this.x -= other.x;
        this.y -= other.y;
        this.z = other.z;
        return this;
    }

    public float length() {
        float l = this.x * this.x + this.y * this.y;
        return (float)Math.sqrt(l);
    }

    public Vec3 midpoint(Vec3 other) {
        return this.plus(other).timesEquals(0.5f);
    }

    public Vec3 midpoint(Vec3 a, Vec3 b) {
        return this.plus(a).plusEquals(b).timesEquals(0.33333334f);
    }

    public float distanceTo(Vec3 other) {
        float dx = this.x - other.x;
        float dy = this.y - other.y;
        float l = dx * dx + dy * dy;
        return (float)Math.sqrt(l);
    }

    public Vec3 normalized() {
        return this.clone().normalizedEquals();
    }

    public Vec3 normalizedEquals() {
        float l = this.length();
        if (l != 0.0f) {
            this.x /= l;
            this.y /= l;
        }
        return this;
    }

    public float dot(Vec3 other) {
        return this.x * other.x + this.y * other.y;
    }

    public float angleDegrees() {
        return (float)((double)this.angleRadians() * 57.29577951308232);
    }

    public float angleRadians() {
        float angle = (float)Math.atan2(this.y, this.x);
        return -angle;
    }

    public static Vec3 normFromRadians(float radians) {
        Vec3 test = new Vec3(1.0f, 0.0f, 0.0f);
        Vec3 out = test.rotateR(radians);
        return out.normalizedEquals();
    }

    public static Vec3 normFromDegrees(float degrees) {
        return Vec3.normFromRadians(degrees * (float)Math.PI / 180.0f);
    }

    public Vec3 rotateR(float radians) {
        return this.clone().rotateREquals(radians);
    }

    public Vec3 rotateREquals(float radians) {
        float old_x = this.x;
        this.x = old_x * (float)Math.cos(radians) - this.y * (float)Math.sin(radians);
        this.y = old_x * (float)Math.sin(radians) + this.y * (float)Math.cos(radians);
        return this;
    }

    public Vec3 rotateD(float degrees) {
        return this.rotateR(degrees * (float)Math.PI / 180.0f);
    }

    public Vec3 times(Mat4 mat) {
        return this.clone().timesEquals(mat);
    }

    public Vec3 timesEquals(Mat4 mat) {
        float[][] matrix = mat.getArray();
        float tx = this.x * matrix[0][0] + this.y * matrix[0][1] + this.z * matrix[0][2] + matrix[0][3];
        float ty = this.x * matrix[1][0] + this.y * matrix[1][1] + this.z * matrix[1][2] + matrix[1][3];
        float tz = this.x * matrix[2][0] + this.y * matrix[2][1] + this.z * matrix[2][2] + matrix[2][3];
        this.x = tx;
        this.y = ty;
        this.z = tz;
        return this;
    }

    public String toString() {
        return "[" + this.x + "," + this.y + "," + this.z + "]";
    }

    public Vec3 clone() {
        return new Vec3(this);
    }

    public static Vec3 closestPointOnSegment(Vec3 p, Vec3 p1, Vec3 p2) {
        Vec3 dir;
        Vec3 diff = p.minus(p1);
        float t = diff.dot(dir = p2.minus(p1)) / dir.dot(dir);
        if (t <= 0.0f) {
            return p1;
        }
        if (t >= 1.0f) {
            return p2;
        }
        return p1.plus(dir.times(t));
    }
}

