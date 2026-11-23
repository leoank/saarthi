package me.leoank.saarthi

import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.speech.RecognitionListener
import android.speech.RecognizerIntent
import android.speech.SpeechRecognizer
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext

/** Dictation into the prompt (ADR-0070, ADR-0073, ADR-0074): Android's own recognizer, on the
 *  device where the phone has one. A tap on the mic listens until the next tap; pressing and
 *  holding listens until it is let go. Where the recognizer has a continuous session
 *  (Android 13+) it listens through pauses; elsewhere it ends at each pause and is started
 *  again at once. Each stretch it heard goes to [heard]; nothing is sent by itself. */
class Dictation(private val context: Context, private val heard: (String) -> Unit) {
    var listening by mutableStateOf(false)
        private set
    /** The words of the stretch being heard, not yet put in. */
    var partial by mutableStateOf("")
        private set
    /** How loud the voice is, 0 to 1, for the mic's ring. */
    var level by mutableStateOf(0f)
        private set
    /** Listening until the next tap, rather than while held. */
    var latched by mutableStateOf(false)
        private set
    /** Why the last try gave nothing, until the next. */
    var problem by mutableStateOf<String?>(null)
        private set

    fun clearProblem() {
        problem = null
    }

    val available: Boolean = SpeechRecognizer.isRecognitionAvailable(context)
    private var recognizer: SpeechRecognizer? = null

    /** Whether to keep listening: from a press until the stop. */
    private var on = false
    /** Whether anything was heard since it started. */
    private var heardAny = false
    /** A press while latched: its lift stops it. */
    private var stopOnLift = false
    /** The continuous session, until the recognizer turns it down. */
    private var segmented = Build.VERSION.SDK_INT >= 33

    /** The mic pressed: listening starts, or, latched, the lift will stop it. */
    fun press() {
        if (on) {
            stopOnLift = true
            return
        }
        on = true
        latched = false
        heardAny = false
        problem = null
        listen()
    }

    /** The mic lifted after [heldMs]: a short tap latches; a hold, or a tap while latched,
     *  stops, and what was heard so far still comes. */
    fun lift(heldMs: Long) {
        if (!on) return
        when {
            stopOnLift -> stop()
            heldMs < TAP_MS -> latched = true
            else -> stop()
        }
        stopOnLift = false
    }

    private fun stop() {
        on = false
        latched = false
        if (listening) recognizer?.stopListening()
    }

    private fun listen() {
        val r = recognizer ?: create().also { recognizer = it }
        val intent = Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH).apply {
            putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
            putExtra(RecognizerIntent.EXTRA_PARTIAL_RESULTS, true)
            putExtra(RecognizerIntent.EXTRA_CALLING_PACKAGE, context.packageName)
            if (segmented && Build.VERSION.SDK_INT >= 33) {
                // One session for as long as it is on, its stretches coming as segments; it
                // lasts at least this long, and stopListening ends it before.
                putExtra(RecognizerIntent.EXTRA_SEGMENTED_SESSION, RecognizerIntent.EXTRA_SPEECH_INPUT_MINIMUM_LENGTH_MILLIS)
                putExtra(RecognizerIntent.EXTRA_SPEECH_INPUT_MINIMUM_LENGTH_MILLIS, SESSION_MS)
            } else {
                // Longer pauses before it ends, where the recognizer heeds them.
                putExtra(RecognizerIntent.EXTRA_SPEECH_INPUT_COMPLETE_SILENCE_LENGTH_MILLIS, 6000L)
                putExtra(RecognizerIntent.EXTRA_SPEECH_INPUT_POSSIBLY_COMPLETE_SILENCE_LENGTH_MILLIS, 6000L)
            }
        }
        partial = ""
        listening = true
        r.startListening(intent)
    }

    /** A stretch of words heard: put in. */
    private fun take(results: Bundle?) {
        val words = results?.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)?.firstOrNull()?.takeIf { it.isNotBlank() }
        partial = ""
        if (words != null) {
            heardAny = true
            heard(words)
        }
    }

    /** Listening ended: on again while still on, else done. */
    private fun ended() {
        partial = ""
        level = 0f
        if (on) listen() else {
            listening = false
            latched = false
            if (!heardAny) problem = "Heard nothing"
        }
    }

    fun release() {
        on = false
        recognizer?.destroy()
        recognizer = null
        listening = false
        latched = false
    }

    /** The on-device recognizer where there is one, so the voice stays on the phone. */
    private fun create(): SpeechRecognizer {
        val local = Build.VERSION.SDK_INT >= 31 && SpeechRecognizer.isOnDeviceRecognitionAvailable(context)
        val r = if (local) SpeechRecognizer.createOnDeviceSpeechRecognizer(context) else SpeechRecognizer.createSpeechRecognizer(context)
        r.setRecognitionListener(object : RecognitionListener {
            override fun onReadyForSpeech(params: Bundle?) {}
            override fun onBeginningOfSpeech() {}
            override fun onRmsChanged(rmsdB: Float) {
                level = ((rmsdB + 2f) / 12f).coerceIn(0f, 1f)
            }
            override fun onBufferReceived(buffer: ByteArray?) {}
            override fun onEndOfSpeech() {}
            override fun onError(error: Int) {
                when (error) {
                    // A pause with nothing said: listen on while on.
                    SpeechRecognizer.ERROR_NO_MATCH, SpeechRecognizer.ERROR_SPEECH_TIMEOUT -> return ended()
                    // Stopped by us.
                    SpeechRecognizer.ERROR_CLIENT -> {
                        on = false
                        return ended()
                    }
                }
                // A recognizer without the continuous session: the restarting way instead.
                if (segmented && !heardAny && on && (error == SpeechRecognizer.ERROR_LANGUAGE_NOT_SUPPORTED || error == SpeechRecognizer.ERROR_CANNOT_CHECK_SUPPORT || error == SpeechRecognizer.ERROR_LANGUAGE_UNAVAILABLE)) {
                    segmented = false
                    return listen()
                }
                on = false
                ended()
                problem = when (error) {
                    SpeechRecognizer.ERROR_INSUFFICIENT_PERMISSIONS -> "No microphone permission"
                    SpeechRecognizer.ERROR_NETWORK, SpeechRecognizer.ERROR_NETWORK_TIMEOUT -> "The recognizer needs the network"
                    SpeechRecognizer.ERROR_RECOGNIZER_BUSY -> "The recognizer is busy"
                    else -> "The recognizer stopped (error $error)"
                }
            }
            override fun onResults(results: Bundle?) {
                take(results)
                ended()
            }
            override fun onPartialResults(partialResults: Bundle?) {
                partialResults?.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)?.firstOrNull()?.let { partial = it }
            }
            // The continuous session's stretches, and its end.
            override fun onSegmentResults(segmentResults: Bundle) = take(segmentResults)
            override fun onEndOfSegmentedSession() = ended()
            override fun onEvent(eventType: Int, params: Bundle?) {}
        })
        return r
    }

    private companion object {
        /** A press shorter than this is a tap. */
        const val TAP_MS = 350L
        /** How long a continuous session may run before it is started again. */
        const val SESSION_MS = 10 * 60 * 1000L
    }
}

/** A [Dictation] for as long as the caller stays composed. */
@Composable
fun rememberDictation(heard: (String) -> Unit): Dictation {
    val context = LocalContext.current
    val latest by rememberUpdatedState(heard)
    val d = remember { Dictation(context.applicationContext) { latest(it) } }
    DisposableEffect(d) { onDispose { d.release() } }
    return d
}

/** [words] put into [text] at [at], with a space on either side where it would run on. */
fun insertWords(text: String, at: Int, words: String): Pair<String, Int> {
    val i = at.coerceIn(0, text.length)
    val before = if (i > 0 && !text[i - 1].isWhitespace()) " " else ""
    val after = if (i < text.length && !text[i].isWhitespace()) " " else ""
    val put = before + words.trim() + after
    return text.substring(0, i) + put + text.substring(i) to i + put.length - after.length
}
