/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 */
package UWBGL_JavaOpenGL;

import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingLine;
import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingList;
import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingSphere;
import UWBGL_JavaOpenGL.BoundingVolumes.UWBGL_BoundingVolume;
import UWBGL_JavaOpenGL.math3d.Vec3;

public class UWBGL_Common {
    public static final float FLOAT_VERY_LARGE = 1.0E9f;
    public static final float FLOAT_VERY_SMALL = -1.0E9f;
    public static final float FLOAT_ZERO_TOLERANCE = 1.0E-9f;
    public static final float PI = (float)Math.PI;

    public static Vec3 findIntersectLineList(Vec3 head, Vec3 tail, UWBGL_BoundingList list) {
        float distance_to_nearest = Float.POSITIVE_INFINITY;
        float nearest_radius = 0.0f;
        Vec3 nearest_center = new Vec3();
        block4: for (int i = 0; i < list.size(); ++i) {
            UWBGL_BoundingVolume bound = list.get(i);
            switch (bound.getType()) {
                case Sphere: {
                    float distance;
                    UWBGL_BoundingSphere cur_circle = (UWBGL_BoundingSphere)bound;
                    if (!UWBGL_Common.intersectLineSphere(head, tail, cur_circle.getCenter(), cur_circle.getRadius()) || !((distance = head.distanceTo(cur_circle.getCenter()) - cur_circle.getRadius()) < distance_to_nearest)) continue block4;
                    distance_to_nearest = distance;
                    nearest_radius = cur_circle.getRadius();
                    nearest_center = cur_circle.getCenter();
                    continue block4;
                }
                case Line: {
                    UWBGL_BoundingLine cur_line = (UWBGL_BoundingLine)bound;
                    if (!UWBGL_Common.intersectLineLine(head, tail, cur_line.getHead(), cur_line.getTail())) continue block4;
                    float dist_to_head = head.distanceTo(cur_line.getHead());
                    float dist_to_tail = head.distanceTo(cur_line.getTail());
                    if (dist_to_head < distance_to_nearest) {
                        distance_to_nearest = dist_to_head;
                        nearest_center = cur_line.getCenter();
                        nearest_radius = cur_line.getLength() * 0.5f;
                    }
                    if (!(dist_to_tail < distance_to_nearest)) continue block4;
                    distance_to_nearest = dist_to_head;
                    nearest_center = cur_line.getCenter();
                    nearest_radius = cur_line.getLength() * 0.5f;
                    continue block4;
                }
                default: {
                    System.err.println("UWBGL_Common->findIntersectLineList: unhandled type: " + (Object)((Object)bound.getType()));
                    System.exit(-1);
                }
            }
        }
        if (distance_to_nearest == Float.POSITIVE_INFINITY) {
            System.err.println("UWBGL_Common->findIntersectLineList: none of the bounds in the list are intersecting the line!");
            System.exit(-1);
        }
        return UWBGL_Common.findIntersectLineSphere(head, tail, nearest_center, nearest_radius);
    }

    public static Vec3 findIntersectLineSphere(Vec3 head, Vec3 tail, Vec3 center, float radius) {
        float dx = head.x - tail.x;
        float dy = head.y - tail.y;
        float A = dx * dx + dy * dy;
        float B = 2.0f * (dx * (head.x - center.x) + dy * (head.y - center.y));
        float C = (head.x - center.x) * (head.x - center.x) + (head.y - center.y) * (head.y - center.y) - radius * radius;
        float det = B * B - 4.0f * A * C;
        if ((double)A <= 1.0E-6 || det < 0.0f) {
            return new Vec3();
        }
        if (det > 0.0f) {
            float t = (-B + (float)Math.sqrt(det)) / (2.0f * A);
            Vec3 int0 = new Vec3(head.x + t * dx, head.y + t * dy);
            t = (-B - (float)Math.sqrt(det)) / (2.0f * A);
            Vec3 int1 = new Vec3(head.x + t * dx, head.y + t * dy);
            if (int0.distanceTo(head) < int1.distanceTo(head)) {
                return int0;
            }
            return int1;
        }
        float t = -B / (2.0f * A);
        return new Vec3(head.x + t * dx, head.y + t * dy);
    }

    public static boolean intersectLineBox(Vec3 m_head, Vec3 m_tail, Vec3 min, Vec3 max) {
        throw new UnsupportedOperationException("Not yet implemented");
    }

    public static boolean intersectLineLine(Vec3 head0, Vec3 tail0, Vec3 head1, Vec3 tail1) {
        Vec3 intersect = UWBGL_Common.findIntersectLineLine(head0, tail0, head1, tail1);
        if (intersect == null) {
            return false;
        }
        if (intersect.x >= head0.x && intersect.x <= tail0.x && intersect.y >= head0.y && intersect.y <= tail0.y) {
            return true;
        }
        return intersect.x >= head1.x && intersect.x <= tail1.x && intersect.y >= head1.y && intersect.y <= tail1.y;
    }

    public static Vec3 findIntersectLineLine(Vec3 head0, Vec3 tail0, Vec3 head1, Vec3 tail1) {
        float a = head0.x * tail0.y - head0.y * tail0.x;
        float b = head1.x * tail1.y - head1.y * tail1.x;
        float c = head0.x - tail0.x;
        float f = head1.y - tail1.y;
        float d = head1.x - tail1.x;
        float e = head0.y - tail0.y;
        float bottom = c * f - d * e;
        if (bottom == 0.0f) {
            return null;
        }
        float x_top = a * d - b * c;
        float y_top = a * f - b * e;
        return new Vec3(x_top / bottom, y_top / bottom);
    }

    public static boolean intersectLineSphere(Vec3 head, Vec3 tail, Vec3 center, float radius) {
        Vec3 normal_root = Vec3.closestPointOnSegment(center, head, tail);
        float shortest_distance = normal_root.distanceTo(center);
        return shortest_distance < radius;
    }

    public static float toRadians(float degree) {
        return degree * ((float)Math.PI / 180);
    }

    public static float toDegrees(float radian) {
        return radian * 57.295776f;
    }

    public static boolean intersectSphereSphere(Vec3 center1, float radius1, Vec3 center2, float radius2) {
        float distance = center1.distanceTo(center2);
        return !(distance > radius1 + radius2);
    }

    public static boolean intersectBoxBox(Vec3 box1_min, Vec3 box1_max, Vec3 box2_min, Vec3 box2_max) {
        boolean zOutside;
        boolean yOutside;
        boolean xOutside;
        boolean bl = xOutside = box1_min.x > box2_max.x || box1_max.x < box2_min.x;
        if (xOutside) {
            return false;
        }
        boolean bl2 = yOutside = box1_min.y > box2_max.y || box1_max.y < box2_min.y;
        if (yOutside) {
            return false;
        }
        boolean bl3 = zOutside = box1_min.z > box2_max.z || box1_max.z < box2_min.z;
        return !zOutside;
    }

    public static boolean intersectSphereBox(Vec3 s_center, float s_radius, Vec3 b_min, Vec3 b_max) {
        Vec3 otherCenter = b_min.midpoint(b_max);
        double dx = otherCenter.x - s_center.x;
        double dy = otherCenter.y - s_center.y;
        double theta = Math.atan2(dy, dx);
        Vec3 closest_point = new Vec3(s_center.x + (float)(Math.cos(theta) * (double)s_radius), s_center.y + (float)(Math.sin(theta) * (double)s_radius), 0.0f);
        return UWBGL_Common.containsPoint(b_min, b_max, closest_point);
    }

    public static boolean containsPoint(Vec3 min, Vec3 max, Vec3 point) {
        boolean zInside;
        boolean yInside;
        boolean xInside;
        boolean bl = xInside = point.x >= min.x && point.x <= max.x;
        if (!xInside) {
            return false;
        }
        boolean bl2 = yInside = point.y >= min.y && point.y <= max.y;
        if (!yInside) {
            return false;
        }
        boolean bl3 = zInside = point.z >= min.z && point.z <= max.z;
        return zInside;
    }
}

