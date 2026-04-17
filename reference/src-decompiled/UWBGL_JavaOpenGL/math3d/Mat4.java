/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 */
package UWBGL_JavaOpenGL.math3d;

import UWBGL_JavaOpenGL.math3d.Vec3;

public class Mat4
implements Cloneable {
    private float[][] A;
    private int m;
    private int n;

    public Mat4() {
        this.m = 4;
        this.n = 4;
        this.A = new float[this.m][this.n];
    }

    public Mat4(float s) {
        this();
        for (int i = 0; i < this.m; ++i) {
            for (int j = 0; j < this.n; ++j) {
                this.A[i][j] = s;
            }
        }
    }

    public Mat4(float[][] A) {
        this.m = A.length;
        this.n = A[0].length;
        if (this.m != 4 && this.n != 4) {
            throw new IllegalArgumentException("Only 4x4 matrix supported");
        }
        for (int i = 0; i < this.m; ++i) {
            if (A[i].length == this.n) continue;
            throw new IllegalArgumentException("All rows must have the same length.");
        }
        this.A = A;
    }

    public Mat4(float[] vals, int m) {
        this.m = m;
        int n = this.n = m != 0 ? vals.length / m : 0;
        if (m * this.n != vals.length) {
            throw new IllegalArgumentException("Array length must be a multiple of m.");
        }
        if (m != 4 && this.n != 4) {
            throw new IllegalArgumentException("Only 4x4 matrix supported");
        }
        this.A = new float[m][this.n];
        for (int i = 0; i < m; ++i) {
            for (int j = 0; j < this.n; ++j) {
                this.A[i][j] = vals[i + j * m];
            }
        }
    }

    public static Mat4 identity() {
        Mat4 A = new Mat4();
        float[][] X = A.getArray();
        for (int i = 0; i < 4; ++i) {
            for (int j = 0; j < 4; ++j) {
                X[i][j] = i == j ? 1.0f : 0.0f;
            }
        }
        return A;
    }

    public static Mat4 constructWithCopy(float[][] A) {
        int m = A.length;
        int n = A[0].length;
        if (m != 4 && n != 4) {
            throw new IllegalArgumentException("Only 4x4 matrix supported");
        }
        Mat4 X = new Mat4();
        float[][] C = X.getArray();
        for (int i = 0; i < m; ++i) {
            if (A[i].length != n) {
                throw new IllegalArgumentException("All rows must have the same length.");
            }
            for (int j = 0; j < n; ++j) {
                C[i][j] = A[i][j];
            }
        }
        return X;
    }

    public Mat4 copy() {
        Mat4 X = new Mat4();
        float[][] C = X.getArray();
        for (int i = 0; i < this.m; ++i) {
            for (int j = 0; j < this.n; ++j) {
                C[i][j] = this.A[i][j];
            }
        }
        return X;
    }

    public Object clone() {
        return this.copy();
    }

    public float[][] getArray() {
        return this.A;
    }

    public float[][] getArrayCopy() {
        float[][] C = new float[this.m][this.n];
        for (int i = 0; i < this.m; ++i) {
            for (int j = 0; j < this.n; ++j) {
                C[i][j] = this.A[i][j];
            }
        }
        return C;
    }

    public int getRowDimension() {
        return this.m;
    }

    public int getColumnDimension() {
        return this.n;
    }

    public float get(int i, int j) {
        return this.A[i][j];
    }

    public void set(int i, int j, float s) {
        this.A[i][j] = s;
    }

    public void set(int i, int j, double s) {
        this.A[i][j] = (float)s;
    }

    public Mat4 transpose() {
        Mat4 X = new Mat4();
        float[][] C = X.getArray();
        for (int i = 0; i < this.m; ++i) {
            for (int j = 0; j < this.n; ++j) {
                C[j][i] = this.A[i][j];
            }
        }
        return X;
    }

    public Mat4 uminus() {
        Mat4 X = new Mat4();
        float[][] C = X.getArray();
        for (int i = 0; i < this.m; ++i) {
            for (int j = 0; j < this.n; ++j) {
                C[i][j] = -this.A[i][j];
            }
        }
        return X;
    }

    public Mat4 plus(Mat4 B) {
        this.checkMatrixDimensions(B);
        Mat4 X = new Mat4();
        float[][] C = X.getArray();
        for (int i = 0; i < this.m; ++i) {
            for (int j = 0; j < this.n; ++j) {
                C[i][j] = this.A[i][j] + B.A[i][j];
            }
        }
        return X;
    }

    public Mat4 plusEquals(Mat4 B) {
        this.checkMatrixDimensions(B);
        for (int i = 0; i < this.m; ++i) {
            for (int j = 0; j < this.n; ++j) {
                this.A[i][j] = this.A[i][j] + B.A[i][j];
            }
        }
        return this;
    }

    public Mat4 minus(Mat4 B) {
        this.checkMatrixDimensions(B);
        Mat4 X = new Mat4();
        float[][] C = X.getArray();
        for (int i = 0; i < this.m; ++i) {
            for (int j = 0; j < this.n; ++j) {
                C[i][j] = this.A[i][j] - B.A[i][j];
            }
        }
        return X;
    }

    public Mat4 minusEquals(Mat4 B) {
        this.checkMatrixDimensions(B);
        for (int i = 0; i < this.m; ++i) {
            for (int j = 0; j < this.n; ++j) {
                this.A[i][j] = this.A[i][j] - B.A[i][j];
            }
        }
        return this;
    }

    public Mat4 arrayTimes(Mat4 B) {
        this.checkMatrixDimensions(B);
        Mat4 X = new Mat4();
        float[][] C = X.getArray();
        for (int i = 0; i < this.m; ++i) {
            for (int j = 0; j < this.n; ++j) {
                C[i][j] = this.A[i][j] * B.A[i][j];
            }
        }
        return X;
    }

    public Mat4 arrayTimesEquals(Mat4 B) {
        this.checkMatrixDimensions(B);
        for (int i = 0; i < this.m; ++i) {
            for (int j = 0; j < this.n; ++j) {
                this.A[i][j] = this.A[i][j] * B.A[i][j];
            }
        }
        return this;
    }

    public Mat4 arrayRightDivide(Mat4 B) {
        this.checkMatrixDimensions(B);
        Mat4 X = new Mat4();
        float[][] C = X.getArray();
        for (int i = 0; i < this.m; ++i) {
            for (int j = 0; j < this.n; ++j) {
                C[i][j] = this.A[i][j] / B.A[i][j];
            }
        }
        return X;
    }

    public Mat4 arrayRightDivideEquals(Mat4 B) {
        this.checkMatrixDimensions(B);
        for (int i = 0; i < this.m; ++i) {
            for (int j = 0; j < this.n; ++j) {
                this.A[i][j] = this.A[i][j] / B.A[i][j];
            }
        }
        return this;
    }

    public Mat4 arrayLeftDivide(Mat4 B) {
        this.checkMatrixDimensions(B);
        Mat4 X = new Mat4();
        float[][] C = X.getArray();
        for (int i = 0; i < this.m; ++i) {
            for (int j = 0; j < this.n; ++j) {
                C[i][j] = B.A[i][j] / this.A[i][j];
            }
        }
        return X;
    }

    public Mat4 arrayLeftDivideEquals(Mat4 B) {
        this.checkMatrixDimensions(B);
        for (int i = 0; i < this.m; ++i) {
            for (int j = 0; j < this.n; ++j) {
                this.A[i][j] = B.A[i][j] / this.A[i][j];
            }
        }
        return this;
    }

    public Mat4 times(float s) {
        Mat4 X = new Mat4();
        float[][] C = X.getArray();
        for (int i = 0; i < this.m; ++i) {
            for (int j = 0; j < this.n; ++j) {
                C[i][j] = s * this.A[i][j];
            }
        }
        return X;
    }

    public Mat4 timesEquals(float s) {
        for (int i = 0; i < this.m; ++i) {
            for (int j = 0; j < this.n; ++j) {
                this.A[i][j] = s * this.A[i][j];
            }
        }
        return this;
    }

    public Mat4 times(Mat4 B) {
        Mat4 X = new Mat4();
        float[][] C = X.getArray();
        float[] Bcolj = new float[this.n];
        for (int j = 0; j < B.n; ++j) {
            for (int k = 0; k < this.n; ++k) {
                Bcolj[k] = B.A[k][j];
            }
            for (int i = 0; i < this.m; ++i) {
                float[] Arowi = this.A[i];
                float s = 0.0f;
                for (int k = 0; k < this.n; ++k) {
                    s += Arowi[k] * Bcolj[k];
                }
                C[i][j] = s;
            }
        }
        return X;
    }

    public static Mat4 transMatrix(Vec3 trans) {
        Mat4 m = Mat4.identity();
        m.set(0, 3, trans.x);
        m.set(1, 3, trans.y);
        m.set(2, 3, trans.z);
        return m;
    }

    public static Mat4 scaleMatrix(Vec3 scale) {
        Mat4 m = Mat4.identity();
        m.set(0, 0, scale.x);
        m.set(1, 1, scale.y);
        m.set(2, 2, scale.z);
        return m;
    }

    public static Mat4 rotMatrix(float radians) {
        Mat4 m = Mat4.identity();
        m.set(0, 0, Math.cos(radians));
        m.set(0, 1, -Math.sin(radians));
        m.set(1, 0, Math.sin(radians));
        m.set(1, 1, Math.cos(radians));
        return m;
    }

    public String toString() {
        return "Matrix: \n" + this.A[0][0] + "," + this.A[0][1] + "," + this.A[0][2] + "," + this.A[0][3] + "\n" + this.A[1][0] + "," + this.A[1][1] + "," + this.A[1][2] + "," + this.A[1][3] + "\n" + this.A[2][0] + "," + this.A[2][1] + "," + this.A[2][2] + "," + this.A[2][3] + "\n" + this.A[3][0] + "," + this.A[3][1] + "," + this.A[3][2] + "," + this.A[3][3] + "\n";
    }

    private void checkMatrixDimensions(Mat4 B) {
        if (B.m != this.m || B.n != this.n) {
            throw new IllegalArgumentException("Matrix dimensions must agree.");
        }
    }
}

