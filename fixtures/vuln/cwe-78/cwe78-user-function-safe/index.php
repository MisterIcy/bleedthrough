<?php

function handle(string $value): void
{
    system('ls -l ' . escapeshellarg($value));
}

handle($_GET['x']);
