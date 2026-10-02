class_name Hud
extends CanvasLayer
## Everything drawn on top of the game: score, wave, health bar, the big
## "Wave N" banner, the controls hint, and the game-over screen.
## It keeps running while the game is paused (process mode "Always"), so
## the game-over screen's button and the R key still work.

## Sent when the player asks to play again (button, R, or Start).
signal restart_requested

## Seconds the controls hint stays on screen at the start.
@export var hint_time: float = 7.0

@onready var _score_label: Label = $ScoreLabel
@onready var _wave_label: Label = $WaveLabel
@onready var _health_bar: ProgressBar = $HealthBar
@onready var _banner: Label = $Banner
@onready var _hint: Label = $Hint
@onready var _game_over: Control = $GameOver
@onready var _final_score: Label = $GameOver/Box/FinalScore
@onready var _restart_button: Button = $GameOver/Box/RestartButton


func _ready() -> void:
	_banner.modulate.a = 0.0
	_game_over.visible = false
	_restart_button.pressed.connect(restart_requested.emit)
	# Fade the controls hint out after a few seconds.
	var tween := create_tween()
	tween.tween_interval(hint_time)
	tween.tween_property(_hint, "modulate:a", 0.0, 1.0)


func _unhandled_input(event: InputEvent) -> void:
	if _game_over.visible and event.is_action_pressed("restart"):
		restart_requested.emit()


func set_score(score: int) -> void:
	_score_label.text = "SCORE  %d" % score


func set_health(current: int, maximum: int) -> void:
	_health_bar.max_value = maximum
	_health_bar.value = current


## Shows "WAVE n" big in the middle, then fades it out.
func show_wave(wave: int) -> void:
	_wave_label.text = "WAVE %d" % wave
	_banner.text = "WAVE %d" % wave
	var tween := create_tween()
	tween.tween_property(_banner, "modulate:a", 1.0, 0.25)
	tween.tween_interval(1.2)
	tween.tween_property(_banner, "modulate:a", 0.0, 0.5)


func show_game_over(score: int, wave: int) -> void:
	_final_score.text = "Score %d  ·  reached wave %d" % [score, wave]
	_game_over.visible = true
	_restart_button.grab_focus()


func is_game_over_visible() -> bool:
	return _game_over.visible
