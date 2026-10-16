<?php

final class Handler
{
    public function handle(string $value): void
    {
        $object = unserialize($value);
    }
}

(new Handler())->handle($_GET['x']);
