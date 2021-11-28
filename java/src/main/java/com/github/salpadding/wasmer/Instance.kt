package com.github.salpadding.wasmer

import java.lang.AutoCloseable
import kotlin.concurrent.withLock

interface Memory {
    fun read(off: Int, len: Int): ByteArray
    fun write(off: Int, buf: ByteArray)
}

/**
 * Instance is not thread safe, dont share Instance object between threads
 */
interface Instance : AutoCloseable {
    fun getMemory(name: String = "memory"): Memory


    /**
     * execute exported function
     */
    fun execute(export: String, args: LongArray = emptyLongs): LongArray

    companion object {
        val emptyLongs = LongArray(0)

        /**
         * create new instance by webassembly byte code, and open options
         */
        @JvmStatic
        fun create(bin: ByteArray, options: Options, hosts: Collection<HostFunction>?): Instance {
            val names = hosts?.map { it.name }?.toTypedArray() ?: emptyArray()
            val hostsArray = hosts?.toTypedArray() ?: emptyArray();

            val sigs = hosts?.map { Natives.encodeSignature(it.signature) }?.toTypedArray() ?: emptyArray()
            val ins = InstanceImpl()
            var insId = -1

            Natives.MUTEX.withLock {
                for (i in 0 until Natives.INSTANCES.size) {
                    if (Natives.INSTANCES[i] == null) {
                        Natives.INSTANCES[i] = ins
                        insId = i
                        break
                    }
                }
            }

            if (insId < 0) {
                throw RuntimeException("failed to create instance, consider close some instances")
            }

            ins.id = insId
            Natives.HOST_FUNCTIONS[insId] = hostsArray

            val descriptor = Natives.createInstance(bin, options.bitmap(), insId, names, sigs)
            ins.descriptor = descriptor
            ins.mem = MemoryImpl(descriptor)
            return ins
        }
    }

    override fun close()
}