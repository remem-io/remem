package io.remem.expo

import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition

class RememModule : Module() {
  override fun definition() = ModuleDefinition {
    Name("Remem")

    Function("hello") {
      "Hello world! 👋"
    }

    AsyncFunction("setValueAsync") { value: String ->
    }
  }
}
