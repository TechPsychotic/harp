package dev.harp.harp

import android.annotation.SuppressLint
import android.bluetooth.BluetoothA2dp
import android.bluetooth.BluetoothAdapter
import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothManager
import android.bluetooth.BluetoothProfile
import android.bluetooth.BluetoothSocket
import android.content.Context
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.ParcelUuid
import io.flutter.plugin.common.EventChannel
import io.flutter.plugin.common.MethodChannel
import org.lsposed.hiddenapibypass.HiddenApiBypass
import java.io.IOException
import java.lang.reflect.Constructor
import java.util.concurrent.Executors

/**
 * The AirPods' control channel (classic L2CAP, PSM 0x1001) for 3D audio's head tracking: the
 * packets go to and from Dart, which hands them to the Rust head tracker.
 *
 * The connection is opened the way Lyre does it: Android 17's public BluetoothSocketSettings
 * first (One UI 9, Android 17+), then Android's hidden L2CAP socket methods and constructors
 * (through HiddenApiBypass). No root. AirPods talk to one app at a time.
 */
@SuppressLint("MissingPermission")
class AirPodsLink(private val context: Context) : EventChannel.StreamHandler {
    companion object {
        val AAP_UUID: ParcelUuid = ParcelUuid.fromString("74ec2172-0bad-4d01-8f77-997b2be0722a")
        const val PSM = 0x1001
        const val TYPE_L2CAP = 3
    }

    private val main = Handler(Looper.getMainLooper())
    private val io = Executors.newSingleThreadExecutor()
    private var sink: EventChannel.EventSink? = null
    private var socket: BluetoothSocket? = null
    private var a2dp: BluetoothA2dp? = null

    private val adapter: BluetoothAdapter?
        get() = (context.getSystemService(Context.BLUETOOTH_SERVICE) as BluetoothManager).adapter

    fun init() {
        try {
            adapter?.getProfileProxy(context, object : BluetoothProfile.ServiceListener {
                override fun onServiceConnected(profile: Int, proxy: BluetoothProfile) {
                    a2dp = proxy as BluetoothA2dp
                }
                override fun onServiceDisconnected(profile: Int) {
                    a2dp = null
                }
            }, BluetoothProfile.A2DP)
        } catch (_: SecurityException) {
        }
    }

    override fun onListen(arguments: Any?, events: EventChannel.EventSink?) {
        sink = events
    }

    override fun onCancel(arguments: Any?) {
        sink = null
    }

    private fun emit(event: Map<String, Any?>) = main.post { sink?.success(event) }

    /** The connected AirPods: bonded, offering Apple's accessory service (or named AirPods). */
    private fun airpods(): BluetoothDevice? {
        val a = adapter ?: return null
        val connected = try { a2dp?.connectedDevices?.map { it.address }?.toSet() } catch (e: SecurityException) { null }
        return try { a.bondedDevices } catch (e: SecurityException) { emptySet() }
            .filter { d -> d.uuids?.contains(AAP_UUID) == true || (d.name ?: "").contains("AirPods", ignoreCase = true) }
            .firstOrNull { d -> connected.isNullOrEmpty() || d.address in connected }
    }

    fun open(result: MethodChannel.Result) {
        io.execute {
            close()
            val device = airpods()
            if (device == null) {
                main.post { result.error("no_airpods", "No AirPods connected", null) }
                return@execute
            }
            try {
                val s = connect(device)
                socket = s
                startReader(s)
                val name = device.name ?: "AirPods"
                main.post { result.success(name) }
            } catch (e: SecurityException) {
                main.post { result.error("permission", "Harp needs the Nearby devices permission", null) }
            } catch (e: Exception) {
                main.post { result.error("open_failed", "Couldn't open the AirPods' controls (close Lyre, CAPod or LibrePods)", e.message) }
            }
        }
    }

    private fun connect(device: BluetoothDevice): BluetoothSocket {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            try { HiddenApiBypass.addHiddenApiExemptions("Landroid/bluetooth/") } catch (_: Throwable) {}
        }
        val failures = mutableListOf<String>()
        val attempts = mutableListOf<Pair<String, () -> BluetoothSocket?>>()
        if (Build.VERSION.SDK_INT >= 37) {
            for (secure in listOf(false, true)) attempts += "public API" to { publicApiSocket(device, secure) }
        }
        for (name in listOf("createInsecureL2capSocket", "createL2capSocket")) {
            attempts += name to {
                val m = BluetoothDevice::class.java.getDeclaredMethod(name, Int::class.javaPrimitiveType)
                m.isAccessible = true
                m.invoke(device, PSM) as BluetoothSocket
            }
        }
        for (c in BluetoothSocket::class.java.declaredConstructors.sortedBy { it.parameterTypes.size }) {
            val args = argumentsFor(c, device) ?: continue
            attempts += "constructor" to {
                c.isAccessible = true
                c.newInstance(*args) as BluetoothSocket
            }
        }
        for ((label, make) in attempts) {
            val s = try {
                make() ?: continue
            } catch (e: Throwable) {
                val cause = (e as? java.lang.reflect.InvocationTargetException)?.cause ?: e
                if (cause is SecurityException) throw cause
                failures += "$label: ${cause.javaClass.simpleName} ${cause.message ?: ""}".trim()
                continue
            }
            try {
                s.connect()
                return s
            } catch (e: IOException) {
                failures += "$label: connect ${e.message ?: ""}".trim()
                try { s.close() } catch (_: IOException) {}
            }
        }
        throw IOException(failures.joinToString(" | ").ifEmpty { "no way to open an L2CAP socket" })
    }

    private fun publicApiSocket(device: BluetoothDevice, secure: Boolean): BluetoothSocket? {
        val settings = Class.forName("android.bluetooth.BluetoothSocketSettings")
        val builderClass = Class.forName("android.bluetooth.BluetoothSocketSettings\$Builder")
        val b = builderClass.getDeclaredConstructor().newInstance()
        builderClass.getMethod("setSocketType", Int::class.javaPrimitiveType).invoke(b, TYPE_L2CAP)
        builderClass.getMethod("setL2capPsm", Int::class.javaPrimitiveType).invoke(b, PSM)
        builderClass.getMethod("setAuthenticationRequired", Boolean::class.javaPrimitiveType).invoke(b, secure)
        builderClass.getMethod("setEncryptionRequired", Boolean::class.javaPrimitiveType).invoke(b, secure)
        val built = builderClass.getMethod("build").invoke(b)
        return BluetoothDevice::class.java.getMethod("createUsingSocketSettings", settings).invoke(device, built) as BluetoothSocket
    }

    private fun argumentsFor(c: Constructor<*>, device: BluetoothDevice): Array<Any?>? {
        val types = c.parameterTypes
        val ints = types.count { it == Int::class.javaPrimitiveType }
        val intValues = when (ints) {
            2 -> listOf(TYPE_L2CAP, PSM)
            3 -> listOf(TYPE_L2CAP, -1, PSM)
            else -> return null
        }
        val known = setOf(Int::class.javaPrimitiveType, Boolean::class.javaPrimitiveType)
        if (types.any { it.isPrimitive && it !in known }) return null
        var i = 0
        var b = 0
        return Array(types.size) { k ->
            when (types[k]) {
                Int::class.javaPrimitiveType -> intValues[i++]
                Boolean::class.javaPrimitiveType -> (b++ < 2)
                BluetoothDevice::class.java -> device
                BluetoothAdapter::class.java -> adapter
                ParcelUuid::class.java -> AAP_UUID
                else -> null
            }
        }
    }

    private fun startReader(s: BluetoothSocket) {
        Thread {
            val buf = ByteArray(4096)
            var reason = "The AirPods disconnected"
            try {
                val input = s.inputStream
                while (true) {
                    val n = input.read(buf)
                    if (n < 0) break
                    if (n > 0) emit(mapOf("type" to "packet", "data" to buf.copyOf(n)))
                }
            } catch (e: IOException) {
                if (socket === s) reason = e.message ?: reason
            }
            if (socket === s) {
                socket = null
                emit(mapOf("type" to "closed", "reason" to reason))
            }
        }.apply { isDaemon = true; name = "harp-airpods"; start() }
    }

    fun send(data: ByteArray, result: MethodChannel.Result) {
        val s = socket ?: return result.error("closed", "Not connected", null)
        io.execute {
            try {
                s.outputStream.write(data)
                s.outputStream.flush()
                main.post { result.success(null) }
            } catch (e: IOException) {
                main.post { result.error("send_failed", e.message, null) }
            }
        }
    }

    fun close() {
        val s = socket ?: return
        socket = null
        try { s.close() } catch (_: IOException) {}
    }
}
