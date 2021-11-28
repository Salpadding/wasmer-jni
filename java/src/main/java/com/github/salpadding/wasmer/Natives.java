package com.github.salpadding.wasmer;

import java.util.List;
import java.util.concurrent.locks.Lock;
import java.util.concurrent.locks.ReentrantLock;

public class Natives {
    static final Lock MUTEX = new ReentrantLock();
    static Instance[] INSTANCES;
    static HostFunction[][] HOST_FUNCTIONS;

    static {
        JNIUtil.loadLibrary("wasmer_jni");
    }

    public static void initialize(int maxInstances) {
        if (INSTANCES != null) return;
        Natives.INSTANCES = new Instance[maxInstances];
        Natives.HOST_FUNCTIONS = new HostFunction[maxInstances][];
    }

    /**
     * create instance and get the descriptor
     */
    static native long createInstance(byte[] module, long options, int instanceId, String[] hostNames, byte[][] signatures);


    static long[] onHostFunction(int instanceId, int hostId, long[] args) {
        Instance ins = INSTANCES[instanceId];
        return Natives.HOST_FUNCTIONS[instanceId][hostId].execute(ins, args);
    }

    /**
     * execute function by function name
     */
    static native long[] execute(long descriptor, String function, long[] args);


    static native byte[] getMemory(long descriptor, int off, int length);


    static native void setMemory(long descriptor, int off, byte[] buf);

    static native void close(long descriptor);


    public static byte[] encodeSignature(List<ValType> params, List<ValType> r) {
        byte[] ret = new byte[1 + params.size()];

        if (r.size() > 1)
            throw new RuntimeException("multi return value is not supported");

        if (r.isEmpty())
            ret[0] = (byte) 0xff;
        else
            ret[0] = r.get(0).value();

        for (int i = 0; i < params.size(); i++) {
            ret[i + 1] = params.get(i).value();
        }

        return ret;
    }
}
