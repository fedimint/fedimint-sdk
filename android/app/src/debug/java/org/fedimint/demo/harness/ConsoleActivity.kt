package org.fedimint.demo.harness

import android.os.Bundle
import android.widget.Button
import android.widget.EditText
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import org.fedimint.demo.R

/**
 * The SDK harness the cross-platform suite in js/e2e drives: a request field,
 * a Run button and a response view, speaking js/e2e/PROTOCOL.md. Every SDK
 * call goes through [Dispatcher]; the state lives in [HarnessSession], so it
 * survives the activity and dies with the process.
 *
 * Debug builds only, with no launcher entry: started by name
 * (`am start -n org.fedimint.demo/.harness.ConsoleActivity`). Its storage is
 * its own directory, apart from the wallet's.
 */
class ConsoleActivity : AppCompatActivity() {
    private lateinit var response: TextView
    private val show: (String) -> Unit = { response.text = it }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_console)
        HarnessSession.useFilesDir(filesDir)

        val request = findViewById<EditText>(R.id.request)
        // Drivers set the text directly; a keyboard would only cover the view.
        request.showSoftInputOnFocus = false
        response = findViewById(R.id.response)
        response.text = HarnessSession.lastResponse
        HarnessSession.listener = show

        findViewById<Button>(R.id.run).setOnClickListener {
            val raw = request.text.toString()
            // An empty field is a repeated tap on a request already taken.
            if (raw.isBlank()) return@setOnClickListener
            request.setText("")
            response.text = HarnessSession.submit(raw)
        }
    }

    override fun onDestroy() {
        if (HarnessSession.listener === show) HarnessSession.listener = null
        super.onDestroy()
    }
}
