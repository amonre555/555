// Pas de console noire en plus de la fenêtre sous Windows (en release).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    yato_lib::run()
}
