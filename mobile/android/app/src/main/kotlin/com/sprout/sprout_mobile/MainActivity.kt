package com.sprout.sprout_mobile

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.ImageDecoder
import android.os.Build
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel
import java.io.ByteArrayOutputStream
import java.nio.ByteBuffer

class MainActivity : FlutterActivity() {
    private var mediaUploadChannel: MethodChannel? = null

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)

        mediaUploadChannel = MethodChannel(
            flutterEngine.dartExecutor.binaryMessenger,
            MEDIA_UPLOAD_CHANNEL,
        ).also { channel ->
            channel.setMethodCallHandler { call, result ->
                when (call.method) {
                    SANITIZE_IMAGE_FOR_UPLOAD_METHOD -> {
                        handleSanitizeImageForUpload(call.arguments, result)
                    }
                    TRANSCODE_IMAGE_TO_JPEG_METHOD -> {
                        handleTranscodeImageToJpeg(call.arguments, result)
                    }
                    else -> result.notImplemented()
                }
            }
        }
    }

    private fun handleSanitizeImageForUpload(
        arguments: Any?,
        result: MethodChannel.Result,
    ) {
        val payload = arguments as? Map<*, *> ?: run {
            invalidArguments(result, "Expected image bytes and mime type.")
            return
        }
        val bytes = payload["bytes"] as? ByteArray ?: run {
            invalidArguments(result, "Expected raw image bytes.")
            return
        }
        val mimeType = payload["mimeType"] as? String ?: run {
            invalidArguments(result, "Expected image mime type.")
            return
        }

        val format = sanitizeCompressFormatFor(mimeType)
        if (format == null) {
            result.error(
                "sanitize_failed",
                "Unable to sanitize picked image.",
                mimeType,
            )
            return
        }

        transformImageBytes(
            bytes = bytes,
            result = result,
            format = format,
            errorCode = "sanitize_failed",
            encodeFailureMessage = "Unable to sanitize picked image.",
            errorDetails = mimeType,
        )
    }

    private fun handleTranscodeImageToJpeg(
        arguments: Any?,
        result: MethodChannel.Result,
    ) {
        val bytes = arguments as? ByteArray ?: run {
            invalidArguments(result, "Expected raw image bytes.")
            return
        }

        transformImageBytes(
            bytes = bytes,
            result = result,
            format = Bitmap.CompressFormat.JPEG,
            errorCode = "transcode_failed",
            encodeFailureMessage = "Unable to convert picked image to JPEG.",
        )
    }

    private fun decodeBitmap(bytes: ByteArray): Bitmap? {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            runCatching {
                val source = ImageDecoder.createSource(ByteBuffer.wrap(bytes))
                ImageDecoder.decodeBitmap(source) { decoder, _, _ ->
                    decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
                }
            }.getOrNull()?.let { return it }
        }

        return BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
    }

    private fun encodeBitmap(
        bitmap: Bitmap,
        format: Bitmap.CompressFormat,
    ): ByteArray? {
        val output = ByteArrayOutputStream()
        val encoded = bitmap.compress(format, 100, output)
        return if (encoded) output.toByteArray() else null
    }

    private fun sanitizeCompressFormatFor(
        mimeType: String,
    ): Bitmap.CompressFormat? {
        return when (mimeType) {
            "image/jpeg" -> Bitmap.CompressFormat.JPEG
            "image/png" -> Bitmap.CompressFormat.PNG
            else -> null
        }
    }

    private fun transformImageBytes(
        bytes: ByteArray,
        result: MethodChannel.Result,
        format: Bitmap.CompressFormat,
        errorCode: String,
        encodeFailureMessage: String,
        errorDetails: Any? = null,
    ) {
        val bitmap = decodeBitmap(bytes) ?: run {
            result.error(
                errorCode,
                "Unable to decode picked image.",
                null,
            )
            return
        }

        val transformedBytes = encodeBitmap(bitmap, format) ?: run {
            result.error(
                errorCode,
                encodeFailureMessage,
                errorDetails,
            )
            return
        }

        result.success(transformedBytes)
    }

    private fun invalidArguments(
        result: MethodChannel.Result,
        message: String,
    ) {
        result.error("invalid_arguments", message, null)
    }

    companion object {
        private const val MEDIA_UPLOAD_CHANNEL = "sprout/media_upload"
        private const val SANITIZE_IMAGE_FOR_UPLOAD_METHOD = "sanitizeImageForUpload"
        private const val TRANSCODE_IMAGE_TO_JPEG_METHOD = "transcodeImageToJpeg"
    }
}
