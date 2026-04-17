/*
 * Decompiled with CFR 0.153-SNAPSHOT (11e700f).
 */
import java.io.ByteArrayInputStream;
import java.io.IOException;
import java.io.InputStream;
import javax.sound.sampled.AudioFormat;
import javax.sound.sampled.AudioInputStream;
import javax.sound.sampled.AudioSystem;
import javax.sound.sampled.Clip;
import javax.sound.sampled.DataLine;
import javax.sound.sampled.LineEvent;
import javax.sound.sampled.LineListener;
import javax.sound.sampled.LineUnavailableException;
import javax.sound.sampled.UnsupportedAudioFileException;

class AudioSample {
    private final Clip clip;

    public AudioSample(InputStream inputStream) throws IOException {
        try {
            InputStream in = AudioSample.ensureMarkResetAvailable(inputStream);
            AudioInputStream audioInputStream = AudioSystem.getAudioInputStream(in);
            AudioFormat audioFormat = audioInputStream.getFormat();
            int size = (int)((long)audioFormat.getFrameSize() * audioInputStream.getFrameLength());
            byte[] audio = new byte[size];
            audioInputStream.read(audio, 0, size);
            DataLine.Info info = new DataLine.Info(Clip.class, audioFormat, size);
            this.clip = (Clip)AudioSystem.getLine(info);
            this.clip.open(audioFormat, audio, 0, size);
        } catch (UnsupportedAudioFileException e) {
            throw new RuntimeException(e);
        } catch (LineUnavailableException e) {
            throw new RuntimeException(e);
        }
    }

    private static InputStream ensureMarkResetAvailable(InputStream inputStream) throws IOException {
        if (inputStream.markSupported()) {
            return inputStream;
        }
        return new ByteArrayInputStream(AudioSample.readEntireStream(inputStream));
    }

    private static byte[] readEntireStream(InputStream inputStream) throws IOException {
        int bytesRead;
        byte[] buffer = new byte[8];
        byte[] data = null;
        int dataLength = 0;
        while ((bytesRead = inputStream.read(buffer)) != -1) {
            data = AudioSample.append(buffer, bytesRead, data, dataLength);
            dataLength += bytesRead;
        }
        return AudioSample.trim(data, dataLength);
    }

    private static byte[] append(byte[] data, int amount, byte[] array, int offset) {
        if (array == null) {
            array = new byte[amount];
        }
        if (offset + amount >= array.length) {
            byte[] newArray = new byte[array.length * 2];
            System.arraycopy(array, 0, newArray, 0, offset);
            array = newArray;
        }
        System.arraycopy(data, 0, array, offset, amount);
        return array;
    }

    private static byte[] trim(byte[] data, int amount) {
        if (data == null) {
            return new byte[amount];
        }
        if (data.length == amount) {
            return data;
        }
        byte[] newArray = new byte[amount];
        System.arraycopy(data, 0, newArray, 0, amount);
        return newArray;
    }

    public void play() {
        this.play(false, false);
    }

    /*
     * WARNING - Removed try catching itself - possible behaviour change.
     */
    public void play(boolean wait, boolean loop) {
        if (wait) {
            WaitUntilFinishedLineListener waitUntilFinishedLineListener = new WaitUntilFinishedLineListener();
            this.clip.addLineListener(waitUntilFinishedLineListener);
            Clip clip = this.clip;
            synchronized (clip) {
                this.play(loop);
                try {
                    this.clip.wait();
                } catch (InterruptedException e) {
                    // empty catch block
                }
            }
            this.clip.removeLineListener(waitUntilFinishedLineListener);
        } else {
            this.play(loop);
        }
    }

    private void play(boolean loop) {
        this.clip.stop();
        this.clip.setFramePosition(0);
        if (loop) {
            this.clip.loop(-1);
        } else {
            this.clip.start();
        }
    }

    public void stop() {
        this.clip.stop();
    }

    private class WaitUntilFinishedLineListener
    implements LineListener {
        /*
         * WARNING - Removed try catching itself - possible behaviour change.
         */
        @Override
        public void update(LineEvent event) {
            if (event.getType().equals(LineEvent.Type.STOP) || event.getType().equals(LineEvent.Type.CLOSE)) {
                Clip clip = AudioSample.this.clip;
                synchronized (clip) {
                    AudioSample.this.clip.notify();
                }
            }
        }
    }
}

