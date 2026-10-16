<?php

final class Handler
{
    public function handle(string $value): void
    {
        readfile('/var/www/files/' . $value);
    }
}

(new Handler())->handle($_GET['x']);
