package org.github.salpadding.wasmer

import java.lang.AutoCloseable
import kotlin.concurrent.withLock

interface Instance : AutoCloseable {
    val id: Int
    fun getMemory(off: Int, len: Int): ByteArray
    fun setMemory(off: Int, buf: ByteArray)
    fun execute(export: String, args: LongArray = emptyLongs): LongArray

    companion object {
        val emptyLongs = LongArray(0)

        @JvmStatic
        fun create(bin: ByteArray, options: Options, hosts: Collection<HostFunction>?): Instance {
            val names = hosts?.map { it.name }?.toTypedArray() ?: emptyArray()
            val m = hosts?.associate { Pair(it.name, it) }

            if (m?.size != hosts?.size) {
                throw RuntimeException("duplicate host function names found in ${hosts?.map{ it.name }}")
            }

            val sigs = hosts?.map { Natives.encodeSignature(it.signature) }?.toTypedArray() ?: emptyArray()
            Natives.GLOBAL_LOCK.withLock {
                val descriptor = Natives.createInstance(bin, options.bitmap(), names, sigs)
                Natives.HOSTS[descriptor] = m ?: emptyMap<Any, Any>()
                val ins = InstanceImpl(descriptor)
                Natives.INSTANCES[descriptor] = ins
                return ins
            }
        }
    }
}