package org.github.salpadding.wasmer;

import kotlin.Pair;

import java.util.List;
import java.util.Map;
import java.util.concurrent.locks.Lock;
import java.util.concurrent.locks.ReentrantLock;

public class Natives {
    public static final int MAX_INSTANCES = 1024;
    public static final InstanceImpl[] INSTANCES = new InstanceImpl[MAX_INSTANCES];
    public static final Map[] HOSTS = new Map[MAX_INSTANCES];
    static final Lock GLOBAL_LOCK = new ReentrantLock();

    static {
        JNIUtil.loadLibrary("wasmer_jni");
    }

    /**
     * create instance and get the descriptor
     */
    static native int createInstance(byte[] module, long options, String[] hostFunctions, byte[][] signatures);


    /**
     * called by dynamic library when host function
     */
    public static long[] onHostFunction(int descriptor, String name, long[] args) {
        HostFunction host = null;
        InstanceImpl ins = null;
        Map<String, HostFunction> map = HOSTS[descriptor];
        if (map != null) {
            host = map.get(name);
        }
        ins = INSTANCES[descriptor];

        if (ins == null || host == null) {
            throw new RuntimeException("host function " + name + " not found");
        }
        return host.execute(ins, args);
    }

    /**
     * execute function by function name
     */
    static native long[] execute(int descriptor, String function, long[] args);


    public static native byte[] getMemory(int descriptor, int off, int length);


    public static native void setMemory(int descriptor, int off, byte[] buf);

    public static native void close(int descriptor);


    public static byte[] encodeSignature(Pair<List<ValType>, List<ValType>> sig) {
        byte[] ret = new byte[1 + sig.component1().size()];

        if(sig.component2().size() > 1)
            throw new RuntimeException("multi return value is not supported");

        if(sig.component2().isEmpty())
            ret[0] = (byte) 0xff;
        else
            ret[0] = sig.component2().get(0).value();

        for(int i = 0; i < sig.component1().size(); i++) {
            ret[i + 1] = sig.component1().get(i).value();
        }

        return ret;
    }
}
