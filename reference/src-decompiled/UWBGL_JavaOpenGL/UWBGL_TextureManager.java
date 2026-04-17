/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 * 
 * Could not load the following classes:
 *  com.sun.opengl.util.texture.Texture
 *  com.sun.opengl.util.texture.TextureIO
 *  javax.media.opengl.GL
 */
package UWBGL_JavaOpenGL;

import com.sun.opengl.util.texture.Texture;
import com.sun.opengl.util.texture.TextureIO;
import java.io.File;
import java.io.IOException;
import java.util.HashMap;
import javax.media.opengl.GL;

public class UWBGL_TextureManager {
    private HashMap<String, Texture> m_Textures = new HashMap();

    public boolean activateTexture(GL gl, String texFileName) {
        Texture texture = this.findTextureResource(gl, texFileName, true);
        if (null == texture) {
            return false;
        }
        texture.enable();
        texture.bind();
        gl.glTexEnvi(8960, 8704, 8449);
        return true;
    }

    public boolean deactivateTexture(GL gl) {
        gl.glDisable(3553);
        return true;
    }

    Texture findTextureResource(GL gl, String texture_name, boolean add_on_failure) {
        if (texture_name.length() == 0) {
            return null;
        }
        Texture texture = this.m_Textures.get(texture_name + gl.hashCode());
        if (texture != null) {
            return texture;
        }
        if (add_on_failure) {
            try {
                texture = TextureIO.newTexture((File)new File(texture_name), (boolean)true);
            } catch (IOException ioe) {
                ioe.printStackTrace();
                return null;
            }
            this.m_Textures.put(texture_name + gl.hashCode(), texture);
            return texture;
        }
        return null;
    }
}

