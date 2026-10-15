<?php

interface Logger
{
    public function log(string $m);
}

$logger = new class (1) implements Logger {
    public $level;

    public function __construct($level = 0)
    {
        $this->level = $level;
    }
    public function log(string $m) {}
};
