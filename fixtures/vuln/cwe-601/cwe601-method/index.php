<?php

final class Handler
{
    public function handle(string $value): void
    {
        header('Location: ' . $value);
    }
}

(new Handler())->handle($_GET['x']);
