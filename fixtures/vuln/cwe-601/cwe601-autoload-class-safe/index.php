<?php

spl_autoload_register(static function (string $class): void {
    require __DIR__ . '/' . $class . '.php';
});

(new Handler())->handle($_GET['x']);
