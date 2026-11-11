package app.yato.player

import android.os.Bundle
import android.webkit.JavascriptInterface
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat

/**
 * L'appli dessine sous les barres système (bord à bord). Les marges de ces barres sont données à
 * l'interface par `window.AndroidInsets` (en px CSS) : la WebView ne les expose pas toujours
 * dans `env(safe-area-inset-*)`. Voir app/src/mobile.ts.
 */
class MainActivity : TauriActivity() {
  @Volatile private var top = 0f
  @Volatile private var bottom = 0f

  inner class Insets {
    @JavascriptInterface fun top(): Float = top
    @JavascriptInterface fun bottom(): Float = bottom
  }

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    // Fond noir : icônes claires dans les barres système.
    WindowCompat.getInsetsController(window, window.decorView).apply {
      isAppearanceLightStatusBars = false
      isAppearanceLightNavigationBars = false
    }
    val density = resources.displayMetrics.density
    ViewCompat.setOnApplyWindowInsetsListener(window.decorView) { view, insets ->
      val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
      top = bars.top / density
      bottom = bars.bottom / density
      ViewCompat.onApplyWindowInsets(view, insets)
    }
  }

  override fun onWebViewCreate(webView: WebView) {
    webView.setBackgroundColor(android.graphics.Color.BLACK)
    webView.addJavascriptInterface(Insets(), "AndroidInsets")
  }
}
